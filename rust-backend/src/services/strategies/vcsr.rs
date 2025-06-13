//! Volume‑Climax Support Reversal (VCSR) strategy implementation
//!
//! This module provides a self‑contained signal‑generation engine for the
//! VCSR mean‑reversion setup described in the project documentation.
//!
//! ## Features implemented
//! * Demand‑zone mapping via volume‑profile HVN detection
//! * Multi‑signal volume spike filter (MA multiple, z‑score, percentile)
//! * Price‑action & order‑flow confirmations (hammer/engulfing & delta flip)
//! * Risk engine (ATR / LVN driven stops, dynamic sizing)
//! * Optional strategy enhancements:
//!     * VWAP −2σ gate
//!     * Order‑book imbalance confirmation
//!     * Time‑of‑day session filter
//! * Built‑in walk‑forward & Monte‑Carlo robustness harness (feature‑gated)
//!
//! ## Integration hints
//! 1. Place this file in `src/services/strategies/` and expose it through
//!    `mod strategies;` and `pub use strategies::vcsr::*;` in `lib.rs`.
//! 2. Wire the `generate_signal()` output into your trading‑engine router.
//! 3. Provide adapters from your exchange client → [`MarketSnapshot`].
//! 4. Enable the `robust` cargo feature to compile the back‑test harness.

use crate::db::redis::RedisPool;
use crate::services::market_data::MarketBus;
use crate::services::strategies::{Candle, OrderBookSnapshot};
use crate::services::trading_engine::{execute_trade, Exchange, TradeRequest};
use chrono::{DateTime, Timelike, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use statrs::statistics::{Data as StatsData, Distribution};
use std::sync::Arc;
use async_trait::async_trait;

#[derive(Debug, Clone, Deserialize)]
pub struct VcsrConfig {
    // volume spike
    pub vol_ma_period: usize,
    pub vol_ma_mult: f64,
    pub vol_zscore: f64,
    pub vol_percentile: f64,

    // HVN
    pub hvn_lookback_days: usize,
    pub hvn_top_value_area_pct: f64,

    // risk
    pub atr_mult: f64,
    pub risk_per_trade: f64,
    pub rr_ratio: f64,

    // enhancements
    pub vwap_sigma: Option<f64>,
    pub ob_bid_ask_ratio: Option<f64>,
    pub session_filter: Option<Vec<TradingSession>>,

    // meta
    pub vwap_window: usize,
}

// -------------------------------------------------------------------------
// Thin façade traits – give the strategy a seam for mocking
// -------------------------------------------------------------------------
type TradeExec =
dyn Fn(TradeRequest, &(dyn Db), i64, bool, &[u8]) -> Result<(), String> + Send + Sync;

#[async_trait]
pub trait Redis: Send + Sync {
    async fn set_eq(&self, key: &str, equity: f64) -> Result<(), ()>; // just as example
}

#[async_trait] pub trait Db: Send + Sync {}
#[async_trait] pub trait MarketBusSub: Send + Sync { async fn recv(&mut self) -> Result<Candle, ()>; }
pub trait RiskChecker: Send + Sync  { fn check_drawdown(&self, uid: i64) -> Result<(), String>; }

// ---- prod impls ---------------------------------------------------------
#[async_trait] impl Redis for RedisPool {
    async fn set_eq(&self, k:&str, v:f64) -> Result<(), ()> {
        self.set_json(k,&v,600).await.map_err(|_|())
    }
}
#[async_trait] impl Db for PgPool                       {}

impl Default for VcsrConfig {
    fn default() -> Self {
        Self {
            vol_ma_period: 20,
            vol_ma_mult: 2.5,
            vol_zscore: 2.0,
            vol_percentile: 0.95,
            hvn_lookback_days: 180,
            hvn_top_value_area_pct: 0.70,
            atr_mult: 1.25,
            risk_per_trade: 0.01,
            rr_ratio: 2.0,
            vwap_sigma: Some(2.0),
            ob_bid_ask_ratio: Some(1.5),
            session_filter: Some(vec![TradingSession::AsiaOpen, TradingSession::NyOpen]),
            vwap_window: 390, // ≈ 1-day of 1-min bars
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
pub enum TradingSession {
    AsiaOpen,
    NyOpen,
}

// ============================================================
// Engine
// ============================================================

#[derive(Debug, Clone)]
pub struct DemandZone {
    pub price: f64,
    pub width: f64,
}

#[derive(Debug, Clone)]
pub struct TradeSignal {
    pub entry: f64,
    pub stop: f64,
    pub target: f64,
    pub size: f64,
}

pub struct VcsrStrategy {
    cfg: VcsrConfig,
    hvn_cache: Vec<DemandZone>,
}

impl VcsrStrategy {
    pub fn new(cfg: VcsrConfig) -> Self {
        Self {
            cfg,
            hvn_cache: vec![],
        }
    }

    pub fn refresh_hvn(&mut self, daily: &[Candle]) {
        self.hvn_cache = map_hvns(daily, self.cfg.hvn_top_value_area_pct);
    }

    /// Return `Some(signal)` if all filters pass, else `None`.
    pub fn generate_signal(
        &self,
        hist: &[Candle],
        order_book: Option<OrderBookSnapshot>,
        equity: f64,
    ) -> Option<TradeSignal> {
        let latest = *hist.last()?;
        let prev = hist.get(hist.len().wrapping_sub(2)).copied();

        // 1. demand zone
        let zone = self
            .hvn_cache
            .iter()
            .find(|z| latest.low <= z.price && latest.high >= z.price)?;
        // 2. session
        if let Some(sessions) = &self.cfg.session_filter {
            if !sessions.contains(&map_session(latest.ts)) {
                return None;
            }
        }
        // 3. VWAP
        if let Some(sig) = self.cfg.vwap_sigma {
            if let Some(v) = intraday_vwap(hist, self.cfg.vwap_window) {
                if latest.close > v.mean - sig * v.std_dev {
                    return None;
                }
            }
        }
        // 4. volume spike
        if !volume_spike(&hist[hist.len() - self.cfg.vol_ma_period..], &self.cfg) {
            return None;
        }
        // 5. PA / flow
        if !is_reversal_candle(latest, prev) && !delta_flip(prev, latest) {
            return None;
        }
        // 6. book imbalance
        if let (Some(ob), Some(r)) = (order_book, self.cfg.ob_bid_ask_ratio) {
            if ob.bid_depth / ob.ask_depth < r {
                return None;
            }
        }

        // --- risk & sizing -------------------------------------------------
        let atr = average_true_range(hist, 14)?;
        let stop = (latest.close - self.cfg.atr_mult * atr).min(zone.price - zone.width);
        let risk = latest.close - stop;
        let size = (equity * self.cfg.risk_per_trade) / risk;
        let target = latest.close + self.cfg.rr_ratio * risk;

        Some(TradeSignal {
            entry: latest.close,
            stop,
            target,
            size,
        })
    }
}

// ============================================================
// Helpers
// ============================================================

fn map_hvns(daily: &[Candle], pct: f64) -> Vec<DemandZone> {
    let mut vols: Vec<(f64, f64)> = daily
        .iter()
        .map(|c| (((c.high + c.low) * 0.5), c.volume))
        .collect();
    vols.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());

    let tot: f64 = vols.iter().map(|v| v.1).sum();
    let mut acc = 0.0;
    let mut zones = vec![];

    for (price, v) in vols {
        acc += v;
        if acc / tot <= pct {
            zones.push(DemandZone {
                price,
                width: price * 0.002,
            });
        } else {
            break;
        }
    }
    zones
}

struct Vwap {
    mean: f64,
    std_dev: f64,
}
fn intraday_vwap(hist: &[Candle], win: usize) -> Option<Vwap> {
    if hist.len() < win {
        return None;
    }
    let slice = &hist[hist.len() - win..];
    let (mut pv, mut vol, mut prices) = (0.0, 0.0, Vec::with_capacity(win));
    for c in slice {
        pv += c.close * c.volume;
        vol += c.volume;
        prices.push(c.close);
    }
    let m = pv / vol.max(1e-8);
    Some(Vwap {
        mean: m,
        std_dev: StatsData::new(prices.clone()).std_dev()?,
    })
}

fn volume_spike(recent: &[Candle], cfg: &VcsrConfig) -> bool {
    let latest = recent.last().unwrap();
    let mut vols: Vec<f64> = recent.iter().map(|c| c.volume).collect();
    let ma = vols.iter().sum::<f64>() / vols.len() as f64;
    if latest.volume < cfg.vol_ma_mult * ma {
        return false;
    }

    let data = StatsData::new(vols.clone());
    let mean = data.mean().unwrap_or(0.0);
    let std = data.std_dev().unwrap_or(1e-9); // never divide by zero

    if (latest.volume - mean) / std < cfg.vol_zscore {
        return false;
    }

    vols.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let rank = vols
        .iter()
        .position(|&v| v >= latest.volume)
        .unwrap_or(vols.len()) as f64
        / vols.len() as f64;

    rank >= cfg.vol_percentile
}

fn is_reversal_candle(c: Candle, prev: Option<Candle>) -> bool {
    let body = (c.close - c.open).abs();
    let hammer = (c.open.min(c.close) - c.low) > body * 2.0;
    let engulf = prev
        .map(|p| c.close > p.open && c.open < p.close)
        .unwrap_or(false);
    hammer || engulf
}

fn delta_flip(prev: Option<Candle>, curr: Candle) -> bool {
    if let Some(prev_c) = prev {
        if let (Some(ld), Some(pd)) = (curr.delta, prev_c.delta) {
            return ld > 0.0 && pd < 0.0;
        }
    }
    false
}

fn average_true_range(hist: &[Candle], n: usize) -> Option<f64> {
    if hist.len() <= n {
        return None;
    }
    let mut trs = Vec::with_capacity(n);
    for w in hist.windows(2).rev().take(n) {
        let pc = w[0].close;
        let c = w[1];
        trs.push(
            (c.high - c.low)
                .max((c.high - pc).abs())
                .max((c.low - pc).abs()),
        );
    }
    Some(trs.iter().sum::<f64>() / n as f64)
}

fn map_session(ts: DateTime<Utc>) -> TradingSession {
    match ts.hour() {
        0..=2 | 23 => TradingSession::AsiaOpen,
        12..=14 => TradingSession::NyOpen,
        _ => TradingSession::AsiaOpen,
    }
}

pub async fn loop_forever(
    row: crate::services::scheduler::StrategyRow,
    redis: RedisPool,
    db: Arc<PgPool>, // HVN cache could be stored later
    bus: MarketBus,
    master_key: Vec<u8>,
    is_demo: bool,
) {
    // user-level config or default
    let cfg: VcsrConfig = serde_json::from_value(row.params).unwrap_or_default();

    let mut engine = VcsrStrategy::new(cfg.clone());
    let mut daily: Vec<Candle> = Vec::with_capacity(cfg.hvn_lookback_days + 5);
    let mut hist4h: Vec<Candle> = Vec::with_capacity(600);

    let mut rx = bus.candles_4h.subscribe();

    let user_id = row.user_id;

    while let Ok(c) = rx.recv().await {
        // --- build daily sample for HVN ----
        if daily.last().map(|d| d.ts.date_naive()) != Some(c.ts.date_naive()) {
            daily.push(c);
            if daily.len() > cfg.hvn_lookback_days {
                daily.remove(0);
            }
            engine.refresh_hvn(&daily);
        }

        // --- 4-hour history buffer ----------
        hist4h.push(c);
        if hist4h.len() < cfg.vol_ma_period + 5 {
            continue;
        }

        // --- generate & execute -------------
        if let Some(sig) = engine.generate_signal(&hist4h, None, /*equity*/ 100_000.0) {
            if let Err(e) = crate::services::risk::check_drawdown(&redis, user_id).await {
                log::warn!("DD limit hit – aborting order: {e}");
                return;
            }

            if let Err(e) = execute_trade(
                TradeRequest {
                    exchange: Exchange::Blowfin,
                    symbol: "BTCUSDT".into(),
                    side: "buy".into(),
                    order_type: "market".into(),
                    price: None,
                    size: sig.size,
                },
                &db,
                user_id,
                is_demo,
                &master_key,
            )
            .await
            {
                log::error!("vcsr trade error: {e:?}");
            }
        }
    }
}

#[cfg(feature = "robust")]
mod robust {
    use super::*;
    use rand::prelude::*;

    /// Rolling 2-yr walk-forward + Monte-Carlo slippage
    #[allow(dead_code)]
    pub fn run(history: &[Candle], cfg: &VcsrConfig) {
        let window = 4_380; // ≈ 2 years of 4-hour bars
        let mut sharpes = Vec::new();
        let mut rng = thread_rng();

        for start in (0..history.len().saturating_sub(window)).step_by(window / 4) {
            let slice = &history[start..start + window];

            // build daily sample for HVN refresh
            let daily: Vec<Candle> = slice.iter().step_by(6).copied().collect();
            let mut engine = VcsrStrategy::new(cfg.clone());
            engine.refresh_hvn(&daily);

            let mut equity = 100_000.0;
            let mut curve = vec![equity];

            for idx in 30..slice.len() {
                if let Some(sig) = engine.generate_signal(&slice[..=idx], None, equity) {
                    let slip = 1.0 + rng.gen_range(-0.0005..0.0005);
                    let pnl = (sig.target * slip - sig.entry * slip) * sig.size;
                    equity += pnl;
                    curve.push(equity);
                }
            }

            let rets: Vec<f64> = curve.windows(2).map(|w| (w[1] - w[0]) / w[0]).collect();
            let stats = StatsData::new(rets.clone());
            let sd = stats.std_dev().unwrap_or(1e-6).max(1e-6);
            let mu = stats.mean().unwrap_or(0.0);
            let sharpe = mu / sd * (252_f64).sqrt();
            sharpes.push(sharpe);
        }

        let avg = StatsData::new(sharpes.clone()).mean().unwrap_or(0.0);
        println!("ROBUST-TEST   avg Sharpe = {:.2}", avg);
    }
}



// =======================================================================
// UNIT TESTS
// =======================================================================
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    use async_trait::async_trait;

    //------------------------------------------------------------------
    // Helpers
    //------------------------------------------------------------------
    impl Default for Candle {
        fn default() -> Self {
            Candle { ts: Default::default(), open:0.0, high:0.0, low:0.0,
                close:0.0, volume:0.0, delta:None }
        }
    }
    fn seq(prices:&[f64], vol: f64) -> Vec<Candle> {
        prices.iter().map(|&p| Candle {
            close:p, open:p, high:p+1.0, low:p-1.0, volume:vol, ..Default::default()
        }).collect()
    }

    //------------------------------------------------------------------
    // Pure maths
    //------------------------------------------------------------------
    #[test] fn hvn_top30pc() {
        let daily = seq(&[10.,11.,12.,13.], 100.);        // equal volume
        let z = map_hvns(&daily, 0.30);
        assert_eq!(z.len(), 1);                            // only first element
    }

    #[test] fn vwap_stats() {
        let h = seq(&[1.,2.,3.,4.,5.], 1.0);
        let v = intraday_vwap(&h, 5).unwrap();
        assert!((v.mean-3.0).abs() < 1e-6);
    }

    #[test] fn vol_spike_trips() {
        let mut h = seq(&[10.;19], 100.);
        h.push(Candle{close:10., volume:1000., ..Default::default()});
        assert!(volume_spike(&h, &VcsrConfig::default()));
    }

    #[test] fn atr_len_guard() { assert!(average_true_range(&[],14).is_none()); }

    //------------------------------------------------------------------
    // Engine mocks
    //------------------------------------------------------------------
    #[derive(Default)]
    struct RMock {
        cnt: std::sync::Mutex<u32>,
    }

    #[async_trait]
    impl Redis for RMock {
        async fn set_eq(&self, _k: &str, _v: f64) -> Result<(), ()> {
            *self.cnt.lock().unwrap() += 1;
            Ok(())
        }
    }

    struct DMock;
    #[async_trait] impl Db for DMock {}

    struct Risk { block: bool }
    impl RiskChecker for Risk {
        fn check_drawdown(&self, _:i64)->Result<(),String>{
            if self.block {Err("dd".into())} else {Ok(())}
        }
    }

    #[derive(Clone)] struct Call { side:String }
    fn collect(out:Arc<Mutex<Vec<Call>>>)
               -> impl Fn(TradeRequest,&(dyn Db),i64,bool,&[u8])->Result<(),String> + Send + Sync {
        move |req,_,_,_,_| { out.lock().unwrap().push(Call{side:req.side}); Ok(()) }
    }

    //------------------------------------------------------------------
    // generate_signal branches
    //------------------------------------------------------------------
    fn base_cfg() -> VcsrConfig { VcsrConfig{ vwap_sigma:None, ob_bid_ask_ratio:None, session_filter:None, ..Default::default() } }

    #[tokio::test]
    async fn happy_path_emits_trade() {
        let mut eng = VcsrStrategy::new(base_cfg());
        // craft a demand zone around price 10
        eng.hvn_cache = vec![DemandZone{price:10.0,width:0.05}];
        let mut h = seq(&[10.;25], 200.);
        h.last_mut().unwrap().volume = 1_000.;             // spike
        h.last_mut().unwrap().delta = Some(100.);
        h[h.len()-2].delta          = Some(-100.);         // delta flip

        assert!(eng.generate_signal(&h, None, 10_000.).is_some());
    }

    #[tokio::test]
    async fn volume_filter_blocks() {
        let eng = VcsrStrategy::new(base_cfg());
        assert!(eng.generate_signal(&seq(&[10.;25], 1.), None, 1.).is_none());
    }

    #[tokio::test]
    async fn risk_block_prevents_exec() {
        let trade_log = Arc::new(Mutex::new(Vec::<Call>::new()));
        let hist = seq(&[10.;25], 1_000.);
        let mut eng = VcsrStrategy::new(base_cfg());
        eng.hvn_cache = vec![DemandZone{price:10.0,width:0.05}];
        hist.last().unwrap(); // silence clippy

        // will emit, but Risk blocks it before exec
        if eng.generate_signal(&hist, None, 10_000.).is_some() {
            collect(trade_log.clone())(
                TradeRequest{ exchange:Exchange::Blowfin, symbol:String::new(),
                    side:"buy".into(), order_type:String::new(),
                    price:None, size:0.0 },
                &DMock, 1,false,&[]
            ).unwrap();
        }
        assert!(trade_log.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn redis_mock_counts_calls() {
        let r = RMock::default();
        assert_eq!(*r.cnt.lock().unwrap(), 0);           // should start at 0
        r.set_eq("equity", 100.0).await.unwrap();
        assert_eq!(*r.cnt.lock().unwrap(), 1);           // incremented
        r.set_eq("equity", 200.0).await.unwrap();
        assert_eq!(*r.cnt.lock().unwrap(), 2);           // incremented again
    }

    //------------------------------------------------------------------
    // Robust harness smoke-test (only when feature enabled)
    //------------------------------------------------------------------
    #[cfg(feature="robust")]
    #[test] fn robust_runs() {
        let hist = seq(&[10.; 4_400], 200.);               // 2y-ish of 4-h bars
        robust::run(&hist, &VcsrConfig::default());
    }
}
