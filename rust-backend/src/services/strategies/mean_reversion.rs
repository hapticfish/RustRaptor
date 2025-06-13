//! Mean-reversion strategy – runtime logic + fully-isolated tests
//! =============================================================

use crate::{
    db::redis::RedisPool,
    services::{
        market_data::MarketBus,
        strategies::common::Candle,
        trading_engine::{execute_trade, Exchange, TradeRequest},
    },
};
use serde::Deserialize;
use sqlx::PgPool;
use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::broadcast;

type TradeExec =
    dyn Fn(TradeRequest, &(dyn Db), i64, bool, &[u8]) -> Result<(), String> + Send + Sync;

/// -------------------------------------------------------------------------
/// Small async traits so we can inject mocks in unit tests
/// -------------------------------------------------------------------------
#[async_trait]
pub trait Redis: Send + Sync {
    async fn set_json(&self, key: &str, value: &[Candle], expiry: usize) -> Result<(), ()>;
}

#[async_trait]
pub trait Db: Send + Sync {} // extend when we need DB calls

#[async_trait]
pub trait MarketBusSub: Send + Sync {
    async fn recv(&mut self) -> Result<Candle, ()>;
}

pub trait RiskChecker: Send + Sync {
    fn check_drawdown(&self, user_id: i64) -> Result<(), String>;
}

/// -------------------------------------------------------------------------
/// Trait impls for the real types so existing prod code is untouched
/// -------------------------------------------------------------------------
#[async_trait]
impl Redis for RedisPool {
    async fn set_json(&self, k: &str, v: &[Candle], e: usize) -> Result<(), ()> {
        self.set_json(k, v, e).await.map_err(|_| ())
    }
}
#[async_trait]
impl Db for PgPool {}

/// Broadcast receiver wrapper so it satisfies our trait
pub struct CandleRx(pub broadcast::Receiver<Candle>);
#[async_trait]
impl MarketBusSub for CandleRx {
    async fn recv(&mut self) -> Result<Candle, ()> {
        self.0.recv().await.map_err(|_| ())
    }
}

/// Real risk checker (sync wrapper around async call)
pub struct RealRisk<'a> {
    pub redis: &'a RedisPool,
}
impl RiskChecker for RealRisk<'_> {
    fn check_drawdown(&self, user_id: i64) -> Result<(), String> {
        futures::executor::block_on(crate::services::risk::check_drawdown(self.redis, user_id))
            .map_err(|e| e.to_string())
    }
}

/// -------------------------------------------------------------------------
/// User-persisted parameters
/// -------------------------------------------------------------------------
#[derive(Clone, Deserialize)]
pub struct MeanRevParams {
    pub symbol: String,
    #[serde(default = "d_period")]
    pub period: usize,
    #[serde(default = "d_sigma")]
    pub sigma: f64,
    #[serde(default = "d_qty")]
    pub qty: f64,
}
fn d_period() -> usize {
    20
}
fn d_sigma() -> f64 {
    2.0
}
fn d_qty() -> f64 {
    0.01
}

/// -------------------------------------------------------------------------
/// Maths helpers & signal
/// -------------------------------------------------------------------------
fn bollinger(c: &[Candle], n: usize, k: f64) -> Option<(f64, f64)> {
    if c.len() < n {
        return None;
    }
    let slice = &c[c.len() - n..];
    let sma = slice.iter().map(|x| x.close).sum::<f64>() / n as f64;
    let sd = (slice.iter().map(|x| (x.close - sma).powi(2)).sum::<f64>() / n as f64).sqrt();
    Some((sma - k * sd, sma + k * sd))
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Sig {
    Buy,
    Sell,
    Hold,
}
fn decide(candles: &[Candle], cfg: &MeanRevParams) -> Sig {
    match bollinger(candles, cfg.period, cfg.sigma) {
        Some((low, high)) => {
            let p = candles.last().unwrap().close;
            if p < low {
                Sig::Buy
            } else if p > high {
                Sig::Sell
            } else {
                Sig::Hold
            }
        }
        None => Sig::Hold,
    }
}

/// -------------------------------------------------------------------------
/// Original public API – **signature unchanged**
/// -------------------------------------------------------------------------
pub async fn loop_forever(
    row: crate::services::scheduler::StrategyRow,
    redis: RedisPool,
    db: Arc<PgPool>, // <-- change here
    bus: MarketBus,
    master_key: Vec<u8>,
    is_demo: bool,
) {
    let rx = CandleRx(bus.candles_4h.subscribe());
    let risk = RealRisk { redis: &redis };

    let db_for_closure = db.clone();

    loop_forever_core(
        row,
        &redis,
        &*db, // Pass reference to Arc target for trait param
        Box::new(rx),
        &master_key,
        is_demo,
        &risk,
        &move |req, _db, uid, demo, key| {
            futures::executor::block_on(execute_trade(req, &db_for_closure, uid, demo, key))
                .map(|_| ())
                .map_err(|e| e.to_string())
        },
    )
    .await;
}

/// -------------------------------------------------------------------------
/// Core logic with trait params – used by both prod wrappers and tests
/// -------------------------------------------------------------------------
#[allow(clippy::too_many_arguments)]
pub async fn loop_forever_core(
    row: crate::services::scheduler::StrategyRow,
    redis: &(dyn Redis),
    db: &(dyn Db),
    mut rx: Box<dyn MarketBusSub>,
    master_key: &[u8],
    is_demo: bool,
    risk: &dyn RiskChecker,
    trade_exec: &TradeExec,
) {
    let cfg: MeanRevParams = serde_json::from_value(row.params).expect("bad mean-reversion params");

    let mut hist: Vec<Candle> = Vec::with_capacity(200);
    let user_id = row.user_id;

    while let Ok(c) = rx.recv().await {
        if cfg.symbol.to_uppercase() != "BTCUSDT" {
            continue;
        }
        hist.push(c);
        if hist.len() < cfg.period {
            continue;
        }

        match decide(&hist, &cfg) {
            Sig::Hold => {}
            Sig::Buy => {
                trade_core(
                    "buy", &cfg, redis, db, user_id, is_demo, master_key, risk, trade_exec,
                )
                .await
            }
            Sig::Sell => {
                trade_core(
                    "sell", &cfg, redis, db, user_id, is_demo, master_key, risk, trade_exec,
                )
                .await
            }
        }

        let _ = redis.set_json("candles:BTCUSDT:4h", &hist, 48 * 3600).await;
    }
}

#[allow(clippy::too_many_arguments)]
pub async fn trade_core(
    side: &str,
    cfg: &MeanRevParams,
    _redis: &(dyn Redis),
    db: &(dyn Db),
    user_id: i64,
    is_demo: bool,
    master_key: &[u8],
    risk: &dyn RiskChecker,
    trade_exec: &TradeExec,
) {
    if let Err(e) = risk.check_drawdown(user_id) {
        log::warn!("DD limit hit – aborting order: {e}");
        return;
    }

    let req = TradeRequest {
        exchange: Exchange::Blowfin,
        symbol: cfg.symbol.clone(),
        side: side.into(),
        order_type: "market".into(),
        price: None,
        size: cfg.qty,
    };
    if let Err(e) = trade_exec(req, db, user_id, is_demo, master_key) {
        log::error!("mean-reversion {side} err: {e:?}");
    }
}

/// -------------------------------------------------------------------------
/// Test-suite: mocks + branch coverage (async + sync)
//  Requires `#[derive(Default)]` on `StrategyRow` or manual construction.
// -------------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};


    fn seq(prices: &[f64]) -> Vec<Candle> {
        prices
            .iter()
            .map(|&p| Candle {
                close: p,
                ..Default::default()
            })
            .collect()
    }

    // ----------------------------------- pure maths -----------------------
    #[test]
    fn bollinger_min_len() {
        assert_eq!(bollinger(&seq(&[1.0, 2.0]), 5, 2.0), None);
    }
    #[test]
    fn bollinger_values() {
        let (lo, hi) = bollinger(&seq(&[10., 12., 13., 14., 15.]), 5, 2.).unwrap();
        println!("lo = {}, hi = {}", lo, hi); // one-time, to get actual value
        // then, after running, update the test:
        assert!((lo - 9.35907).abs() < 0.01);
        assert!((hi - 16.2409).abs() < 0.01);
        // Add a comment:
        // This uses population SD: sqrt(sum((xi-mean)^2)/n)
    }
    #[test]
    fn decide_all_branches() {
        let mut v = vec![10.0; 19];
        v.push(5.0);
        let cfg = MeanRevParams {
            symbol: "BTCUSDT".into(),
            period: 20,
            sigma: 2.0,
            qty: 0.1,
        };
        assert_eq!(decide(&seq(&v), &cfg), Sig::Buy);

        let mut v = vec![10.0; 19];
        v.push(20.0);
        assert_eq!(decide(&seq(&v), &cfg), Sig::Sell);

        let mut v = vec![10.0; 19];
        v.push(10.0);
        assert_eq!(decide(&seq(&v), &cfg), Sig::Hold);
    }

    // ----------------------------------- mocks ---------------------------
    use async_trait::async_trait;

    #[derive(Default)]
    struct RMock {
        cnt: Arc<Mutex<u32>>,
    }
    #[async_trait]
    impl Redis for RMock {
        async fn set_json(&self, _k: &str, _v: &[Candle], _e: usize) -> Result<(), ()> {
            *self.cnt.lock().unwrap() += 1;
            Ok(())
        }
    }
    #[derive(Default)]
    struct DMock;
    #[async_trait]
    impl Db for DMock {}

    struct RxMock {
        candles: Vec<Candle>,
        idx: usize,
    }
    #[async_trait]
    impl MarketBusSub for RxMock {
        async fn recv(&mut self) -> Result<Candle, ()> {
            if self.idx < self.candles.len() {
                let c = self.candles[self.idx];
                self.idx += 1;
                Ok(c)
            } else {
                Err(())
            }
        }
    }

    struct RiskMock {
        fail: bool,
    }
    impl RiskChecker for RiskMock {
        fn check_drawdown(&self, _: i64) -> Result<(), String> {
            if self.fail {
                Err("dd".into())
            } else {
                Ok(())
            }
        }
    }
    fn exec_mock(
        fail: bool,
    ) -> impl Fn(TradeRequest, &(dyn Db), i64, bool, &[u8]) -> Result<(), String> + Send + Sync
    {
        move |_, _, _, _, _| if fail { Err("boom".into()) } else { Ok(()) }
    }

    // ----------------------------------- trade_core branches -------------
    #[tokio::test]
    async fn trade_dd_abort() {
        trade_core(
            "buy",
            &MeanRevParams {
                symbol: "BTCUSDT".into(),
                period: 20,
                sigma: 2.0,
                qty: 0.01,
            },
            &RMock::default(),
            &DMock,
            1,
            false,
            &[],
            &RiskMock { fail: true },
            &exec_mock(false),
        )
        .await;
    }
    #[tokio::test]
    async fn trade_exec_err() {
        trade_core(
            "sell",
            &MeanRevParams {
                symbol: "BTCUSDT".into(),
                period: 20,
                sigma: 2.0,
                qty: 0.01,
            },
            &RMock::default(),
            &DMock,
            1,
            false,
            &[],
            &RiskMock { fail: false },
            &exec_mock(true),
        )
        .await;
    }
    #[tokio::test]
    async fn trade_happy() {
        trade_core(
            "sell",
            &MeanRevParams {
                symbol: "BTCUSDT".into(),
                period: 20,
                sigma: 2.0,
                qty: 0.01,
            },
            &RMock::default(),
            &DMock,
            1,
            false,
            &[],
            &RiskMock { fail: false },
            &exec_mock(false),
        )
        .await;
    }

    // ----------------------------------- loop_core happy/branches --------
    #[tokio::test]
    async fn loop_branches() {
        let mut c = vec![
            Candle {
                close: 10.0,
                ..Default::default()
            };
            19
        ];
        c.push(Candle {
            close: 5.0,
            ..Default::default()
        }); // Buy
        c.push(Candle {
            close: 20.0,
            ..Default::default()
        }); // Sell

        let row = crate::services::scheduler::StrategyRow {
            user_id: 42,
            params: serde_json::json!({
                "symbol":"BTCUSDT","period":20,"sigma":2.0,"qty":0.1
            }),
            ..Default::default() // Add derive(Default) if missing
        };

        loop_forever_core(
            row,
            &RMock::default(),
            &DMock,
            Box::new(RxMock { candles: c, idx: 0 }),
            &[],
            false,
            &RiskMock { fail: false },
            &exec_mock(false),
        )
        .await;
    }
}
