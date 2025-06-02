//! Medium-Term Trend-Following (fast/slow SMA × Donchian breakout)

use chrono::Timelike;
use serde::Deserialize;
use sqlx::PgPool;
use std::sync::Arc;

use crate::{
    db::redis::RedisPool,
    services::{
        market_data::MarketBus,
        strategies::Candle,
        trading_engine::{execute_trade, Exchange, TradeRequest},
    },
};

/// --------------------------------------------------------------------
/// User-persisted parameters (in `user_strategies.params`)
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

/// --------------------------------------------------------------------
/// Tokio task entry-point (called by scheduler)
pub async fn loop_forever(
    row: crate::services::scheduler::StrategyRow,
    redis: RedisPool,
    db: Arc<PgPool>,
    bus: MarketBus,
    master_key: Vec<u8>,
    is_demo: bool,
) {
    let cfg: TrendParams = serde_json::from_value(row.params).expect("bad trend-follow params");

    // rolling buffer of daily candles (keep slow-SMA window + a few extra)
    let mut daily: Vec<Candle> = Vec::with_capacity(cfg.slow as usize + 5);

    //     ↳ subscribe to 1-hour stream
    let mut rx = bus.candles_1h.subscribe();
    let mut agg: Option<Candle> = None;

    let user_id = row.user_id;

    while let Ok(c) = rx.recv().await {
        if cfg.symbol.to_uppercase() != "BTCUSDT" {
            continue; // quick symbol filter until multi-sym support
        }

        // ---------- Hourly → Daily aggregation ----------
        match &mut agg {
            None => agg = Some(c), // first hour of new day
            Some(d) => {
                d.high = d.high.max(c.high);
                d.low = d.low.min(c.low);
                d.close = c.close;
                d.volume += c.volume;
            }
        }

        // new UTC day starts when hour == 0
        if c.ts.hour() == 0 {
            if let Some(finished) = agg.take() {
                daily.push(finished);
                if daily.len() > cfg.slow as usize + 10 {
                    daily.remove(0);
                }
                evaluate(&daily, &cfg, &redis, &db, user_id, &master_key, is_demo).await;
            }
        }
    }
}

/// --------------------------------------------------------------------
/// Signal engine + trade execution
async fn evaluate(
    d: &[Candle],
    cfg: &TrendParams,
    redis: &RedisPool,
    db: &PgPool,
    user_id: i64,
    master_key: &[u8],
    is_demo: bool,
) {
    if d.len() < cfg.slow as usize {
        return;
    }

    // helper slices
    let closes: Vec<f64> = d.iter().map(|x| x.close).collect();
    let highs: Vec<f64> = d.iter().map(|x| x.high).collect();
    let lows: Vec<f64> = d.iter().map(|x| x.low).collect();

    let sma = |v: &[f64]| -> f64 { v.iter().sum::<f64>() / v.len() as f64 };

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

    // --------------- position flag in Redis ---------------
    let pos_key = format!("trendpos:{user_id}");
    let in_pos: bool = redis
        .get_json::<_, bool>(&pos_key) // Result<Option<bool>, _>
        .await
        .ok() // Option<Option<bool>>
        .flatten() // Option<bool>
        .unwrap_or(false);

    match (in_pos, fast > slow, price >= don_h, price <= don_l) {
        // --- Exit ---
        (true, _, _, exit) if exit => {
            trade("sell", cfg, redis, db, user_id, is_demo, master_key).await;
            let _ = redis.set_json(&pos_key, &false, 0).await;
        }
        // --- Entry ---
        (false, true, entry, _) if entry => {
            trade("buy", cfg, redis, db, user_id, is_demo, master_key).await;
            let _ = redis.set_json(&pos_key, &true, 3600 * 24 * 30).await;
        }
        _ => {} // hold
    }
}

async fn trade(
    side: &str,
    cfg: &TrendParams,
    redis: &RedisPool,
    db: &PgPool,
    user_id: i64,
    is_demo: bool,
    master_key: &[u8],
) {
    if let Err(e) = crate::services::risk::check_drawdown(redis, user_id).await {
        log::warn!("DD limit hit – aborting order: {e}");
        return;
    }

    if let Err(e) = execute_trade(
        TradeRequest {
            exchange: Exchange::Blowfin,
            symbol: cfg.symbol.clone(),
            side: side.into(),
            order_type: "market".into(),
            price: None,
            size: cfg.qty,
        },
        db,
        user_id,
        is_demo,
        master_key,
    )
    .await
    {
        log::error!("trend-follow {side} err: {e:?}");
    }
}
