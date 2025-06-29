//! Medium-Term Trend-Following strategy
//! ====================================
//! Fast/Slow SMA × Donchian breakout with Redis
//! position-flag and full unit tests.

use chrono::Timelike;
use serde::Deserialize;
use sqlx::PgPool;
use std::sync::Arc;

use crate::{
    db::redis::RedisPool,
    services::{
        market_data::MarketBus,
        strategies::common::Candle,
        trading_engine::{execute_trade, Exchange, TradeRequest},
    },
};

/// ------------------------------------------------------------
/// User-persisted parameters
/// ------------------------------------------------------------
#[derive(Clone, Deserialize)]
pub struct TrendParams {
    pub symbol: String,
    #[serde(default = "d20")]
    pub fast: u16,
    #[serde(default = "d100")]
    pub slow: u16,
    #[serde(default = "d55")]
    pub don: u16,
    #[serde(default = "dq")]
    pub qty: f64,
}
fn d20() -> u16 {
    20
}
fn d100() -> u16 {
    100
}
fn d55() -> u16 {
    55
}
fn dq() -> f64 {
    0.01
}

/// ------------------------------------------------------------
/// Mini-traits so we can inject mocks in tests
/// ------------------------------------------------------------
use async_trait::async_trait;
type TradeExec =
    dyn Fn(TradeRequest, &(dyn Db), i64, bool, &[u8]) -> Result<(), String> + Send + Sync;

#[async_trait]
pub trait Redis: Send + Sync {
    async fn set_pos_flag(&self, key: &str, value: bool, ttl_secs: usize) -> Result<(), ()>;

    async fn get_pos_flag(&self, key: &str) -> Result<Option<bool>, ()>;
}
#[async_trait]
pub trait Db: Send + Sync {}
#[async_trait]
pub trait MarketBusSub: Send + Sync {
    async fn recv(&mut self) -> Result<Candle, ()>;
}
pub trait RiskChecker: Send + Sync {
    fn check_drawdown(&self, user_id: i64) -> Result<(), String>;
}

/// ---- impls for real types (prod path unchanged) ------------------------
#[async_trait]
impl Redis for RedisPool {
    async fn set_pos_flag(&self, key: &str, value: bool, ttl_secs: usize) -> Result<(), ()> {
        self.set_json(key, &value, ttl_secs).await.map_err(|_| ())
    }

    async fn get_pos_flag(&self, key: &str) -> Result<Option<bool>, ()> {
        self.get_json(key).await.map_err(|_| ())
    }
}
#[async_trait]
impl Db for PgPool {}

use tokio::sync::broadcast;
pub struct CandleRx(pub broadcast::Receiver<Candle>);
#[async_trait]
impl MarketBusSub for CandleRx {
    async fn recv(&mut self) -> Result<Candle, ()> {
        self.0.recv().await.map_err(|_| ())
    }
}

/// Real risk wrapper
pub struct RealRisk<'a> {
    redis: &'a RedisPool,
}
impl RiskChecker for RealRisk<'_> {
    fn check_drawdown(&self, uid: i64) -> Result<(), String> {
        futures::executor::block_on(crate::services::risk::check_drawdown(self.redis, uid))
            .map_err(|e| e.to_string())
    }
}

/// ------------------------------------------------------------
/// Public Tokio task (signature unchanged)
/// ------------------------------------------------------------
pub async fn loop_forever(
    row: crate::services::scheduler::StrategyRow,
    redis: RedisPool,
    db: Arc<PgPool>,
    bus: MarketBus,
    master_key: Vec<u8>,
    is_demo: bool,
) {
    let cfg: TrendParams = serde_json::from_value(row.params).expect("bad trend params");

    let mut daily: Vec<Candle> = Vec::with_capacity(cfg.slow as usize + 5);
    let rx = CandleRx(bus.candles_1h.subscribe());
    let risk = RealRisk { redis: &redis };
    let db_cl = db.clone();

    loop_core(
        cfg,
        &redis,
        &*db,
        Box::new(rx),
        row.user_id,
        &master_key,
        is_demo,
        &risk,
        &move |req, _, uid, demo, key| {
            futures::executor::block_on(execute_trade(req, &db_cl, uid, demo, key))
                .map(|_| ())
                .map_err(|e| e.to_string())
        },
        &mut daily,
    )
    .await;
}

/// ------------------------------------------------------------
/// Core loop – testable & mock-friendly
/// ------------------------------------------------------------
#[allow(clippy::too_many_arguments)]
pub async fn loop_core(
    cfg: TrendParams,
    redis: &(dyn Redis),
    db: &(dyn Db),
    mut rx: Box<dyn MarketBusSub>,
    user_id: i64,
    master_key: &[u8],
    is_demo: bool,
    risk: &dyn RiskChecker,
    trade_exec: &TradeExec,
    daily_buf: &mut Vec<Candle>, // pass mutable buffer so tests can pre-seed
) {
    let mut agg: Option<Candle> = None;

    while let Ok(c) = rx.recv().await {
        if cfg.symbol.to_uppercase() != "BTCUSDT" {
            continue;
        }

        match &mut agg {
            None => agg = Some(c),
            Some(d) => {
                d.high = d.high.max(c.high);
                d.low = d.low.min(c.low);
                d.close = c.close;
                d.volume += c.volume;
            }
        }

        if c.ts.hour() == 0 {
            if let Some(finished) = agg.take() {
                daily_buf.push(finished);
                if daily_buf.len() > cfg.slow as usize + 10 {
                    daily_buf.remove(0);
                }
                evaluate_core(
                    daily_buf, &cfg, redis, db, user_id, master_key, is_demo, risk, trade_exec,
                )
                .await;
            }
        }
    }
}

/// ------------------------------------------------------------
/// Pure evaluate logic (no networking) – unit-test target
/// ------------------------------------------------------------
#[allow(clippy::too_many_arguments)]
pub async fn evaluate_core(
    d: &[Candle],
    cfg: &TrendParams,
    redis: &(dyn Redis),
    db: &(dyn Db),
    user_id: i64,
    master_key: &[u8],
    is_demo: bool,
    risk: &dyn RiskChecker,
    trade_exec: &TradeExec,
) {
    if d.len() < cfg.slow as usize {
        return;
    }

    let closes: Vec<f64> = d.iter().map(|c| c.close).collect();
    let highs: Vec<f64> = d.iter().map(|c| c.high).collect();
    let lows: Vec<f64> = d.iter().map(|c| c.low).collect();

    let sma = |v: &[f64]| v.iter().sum::<f64>() / v.len() as f64;

    let fast = sma(&closes[closes.len() - cfg.fast as usize..]);
    let slow = sma(&closes[closes.len() - cfg.slow as usize..]);

    let don_h = highs
        .iter()
        .rev()
        .take(cfg.don as usize)
        .fold(f64::MIN, |a, &b| a.max(b));
    let don_l = lows
        .iter()
        .rev()
        .take(cfg.don as usize)
        .fold(f64::MAX, |a, &b| a.min(b));
    let price = *closes.last().unwrap();

    let pos_key = format!("trendpos:{user_id}");
    let in_pos: bool = redis
        .get_pos_flag(&pos_key)
        .await
        .ok()
        .flatten()
        .unwrap_or(false);

    match (in_pos, fast > slow, price >= don_h, price <= don_l) {
        // Exit ↓
        (true, _, _, exit) if exit => {
            if risk.check_drawdown(user_id).is_ok() {
                let req = TradeRequest {
                    exchange: Exchange::Blowfin,
                    symbol: cfg.symbol.clone(),
                    side: "sell".into(),
                    order_type: "market".into(),
                    price: None,
                    size: cfg.qty,
                };
                let _ = trade_exec(req, db, user_id, is_demo, master_key);
            }
            let _ = redis.set_pos_flag(&pos_key, false, 0).await;
        }
        // Entry ↑
        (false, true, entry, _) if entry => {
            if risk.check_drawdown(user_id).is_ok() {
                let req = TradeRequest {
                    exchange: Exchange::Blowfin,
                    symbol: cfg.symbol.clone(),
                    side: "buy".into(),
                    order_type: "market".into(),
                    price: None,
                    size: cfg.qty,
                };
                let _ = trade_exec(req, db, user_id, is_demo, master_key);
            }
            let _ = redis.set_pos_flag(&pos_key, true, 3600 * 24 * 30).await;
        }
        _ => {}
    }
}

////////////////////////////////////////////////////////////////
// TEST-SUITE
////////////////////////////////////////////////////////////////
#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use std::sync::{Arc, Mutex};

    fn make(days: usize, price: f64) -> Vec<Candle> {
        (0..days)
            .map(|_| Candle {
                close: price,
                high: price,
                low: price,
                ..Default::default()
            })
            .collect()
    }

    // ---------- redis mock -------------
    #[derive(Default)]
    struct RMock {
        pos: Arc<Mutex<Option<bool>>>,
        sets: Arc<Mutex<u32>>,
    }
    #[async_trait]
    impl Redis for RMock {
        async fn set_pos_flag(&self, _k: &str, v: bool, _e: usize) -> Result<(), ()> {
            *self.pos.lock().unwrap() = Some(v);
            *self.sets.lock().unwrap() += 1;
            Ok(())
        }

        async fn get_pos_flag(&self, _k: &str) -> Result<Option<bool>, ()> {
            Ok(*self.pos.lock().unwrap())
        }
    }

    // ---------- db mock (unit struct) ---
    struct DMock;
    #[async_trait]
    impl Db for DMock {}

    // ---------- risk mock ---------------
    struct Risk {
        fail: bool,
    }
    impl RiskChecker for Risk {
        fn check_drawdown(&self, _: i64) -> Result<(), String> {
            if self.fail {
                Err("dd".into())
            } else {
                Ok(())
            }
        }
    }

    // ---------- trade exec collector ----
    #[derive(Clone)]
    struct Call {
        side: String,
        qty: f64,
    }
    fn collect(
        vec: Arc<Mutex<Vec<Call>>>,
    ) -> impl Fn(TradeRequest, &(dyn Db), i64, bool, &[u8]) -> Result<(), String> + Send + Sync
    {
        move |req, _, _, _, _| {
            vec.lock().unwrap().push(Call {
                side: req.side,
                qty: req.size,
            });
            Ok(())
        }
    }

    // ---------- tests -------------------
    #[tokio::test]
    async fn entry_signal_triggers_buy_and_sets_flag() {
        let cfg = TrendParams {
            symbol: "BTCUSDT".into(),
            fast: 3,
            slow: 5,
            don: 2,
            qty: 0.1,
        };

        // price series makes fast>slow and price == don_h
        let mut hist = make(5, 10.0);
        hist.push(Candle {
            close: 12.0,
            high: 12.0,
            low: 12.0,
            ..Default::default()
        });

        let redis = RMock::default();
        let db = DMock;
        let calls = Arc::new(Mutex::new(Vec::<Call>::new()));

        evaluate_core(
            &hist,
            &cfg,
            &redis,
            &db,
            1,
            &[],
            false,
            &Risk { fail: false },
            &collect(calls.clone()),
        )
        .await;

        assert_eq!(calls.lock().unwrap().len(), 1);
        assert_eq!(*redis.pos.lock().unwrap(), Some(true));
        assert_eq!(calls.lock().unwrap()[0].qty, 0.1);
    }

    #[tokio::test]
    async fn exit_signal_triggers_sell_and_unsets_flag() {
        let cfg = TrendParams {
            symbol: "BTCUSDT".into(),
            fast: 3,
            slow: 5,
            don: 2,
            qty: 0.1,
        };

        // start above don_h to mimic open position then drop below don_l
        let mut hist = make(5, 10.0);
        hist.push(Candle {
            close: 5.0,
            high: 10.0,
            low: 5.0,
            ..Default::default()
        });

        let redis = RMock {
            pos: Arc::new(Mutex::new(Some(true))),
            ..Default::default()
        };
        let db = DMock;
        let calls = Arc::new(Mutex::new(Vec::<Call>::new()));

        evaluate_core(
            &hist,
            &cfg,
            &redis,
            &db,
            1,
            &[],
            false,
            &Risk { fail: false },
            &collect(calls.clone()),
        )
        .await;

        assert_eq!(calls.lock().unwrap()[0].side, "sell");
        assert_eq!(*redis.pos.lock().unwrap(), Some(false));
    }

    #[tokio::test]
    async fn risk_block_prevents_trade() {
        let cfg = TrendParams {
            symbol: "BTCUSDT".into(),
            fast: 3,
            slow: 5,
            don: 2,
            qty: 0.1,
        };
        let hist = make(6, 12.0); // triggers entry

        let redis = RMock::default();
        let db = DMock;
        let calls = Arc::new(Mutex::new(Vec::<Call>::new()));

        evaluate_core(
            &hist,
            &cfg,
            &redis,
            &db,
            1,
            &[],
            false,
            &Risk { fail: true },
            &collect(calls.clone()),
        )
        .await;

        assert!(calls.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn too_few_candles_noop() {
        let cfg = TrendParams {
            symbol: "BTCUSDT".into(),
            fast: 3,
            slow: 5,
            don: 2,
            qty: 0.1,
        };
        let hist = make(3, 10.0);

        let redis = RMock::default();
        let db = DMock;
        let calls = Arc::new(Mutex::new(Vec::<Call>::new()));

        evaluate_core(
            &hist,
            &cfg,
            &redis,
            &db,
            1,
            &[],
            false,
            &Risk { fail: false },
            &collect(calls.clone()),
        )
        .await;

        assert!(calls.lock().unwrap().is_empty());
    }
}
