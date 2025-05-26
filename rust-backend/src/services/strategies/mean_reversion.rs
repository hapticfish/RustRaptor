// src/services/strategies/mean_reversion.rs

use crate::{
    config::settings::Settings,
    db::redis::RedisPool,
    services::{
        market_data::MarketBus,
        strategies::{Candle},
        trading_engine::{execute_trade, Exchange, TradeRequest},
    },
};
// use chrono::{DateTime, Utc};
// use redis::RedisError;
use serde::{Deserialize};
use sqlx::PgPool;
// use crate::utils::errors::TradeError;

/// --------------------------------------------------------------------
/// User-persisted parameters
#[derive(Clone, Deserialize)]
pub struct MeanRevParams {
    pub symbol: String,
    #[serde(default = "d_period")] pub period: usize,
    #[serde(default = "d_sigma")]  pub sigma:  f64,
    #[serde(default = "d_qty")]    pub qty:    f64,
}
fn d_period() -> usize { 20 }
fn d_sigma () -> f64   { 2.0 }
fn d_qty   () -> f64   { 0.01 }

/// --------------------------------------------------------------------
/// Maths helpers
fn bollinger(c: &[Candle], n: usize, k: f64) -> Option<(f64, f64)> {
    if c.len() < n { return None; }
    let slice = &c[c.len() - n..];
    let sma   = slice.iter().map(|x| x.close).sum::<f64>() / n as f64;
    let sd    = (slice.iter().map(|x| (x.close - sma).powi(2)).sum::<f64>() / n as f64).sqrt();
    Some((sma - k * sd, sma + k * sd))
}

#[derive(Clone, Copy)]
enum Sig { Buy, Sell, Hold }

fn decide(candles: &[Candle], cfg: &MeanRevParams) -> Sig {
    match bollinger(candles, cfg.period, cfg.sigma) {
        Some((low, high)) => {
            let p = candles.last().unwrap().close;
            if p < low      { Sig::Buy  }
            else if p > high{ Sig::Sell }
            else            { Sig::Hold }
        }
        None => Sig::Hold,
    }
}

/// --------------------------------------------------------------------
/// Tokio loop (spawned by scheduler)
pub async fn loop_forever(
    row: crate::services::scheduler::StrategyRow,
    redis: RedisPool,
    db: PgPool,
    settings: Settings,
    bus: MarketBus,
    master_key: Vec<u8>,     // Pass master key in at job start
    is_demo: bool,
) {
    let cfg: MeanRevParams =
        serde_json::from_value(row.params).expect("bad mean-reversion params");

    let mut hist: Vec<Candle> = Vec::with_capacity(200);
    let mut rx = bus.candles_4h.subscribe();

    let user_id = row.user_id;

    while let Ok(c) = rx.recv().await {
        // quick single-symbol filter until multi-symbol support added
        if cfg.symbol.to_uppercase() != "BTCUSDT" { continue; }

        hist.push(c);
        if hist.len() < cfg.period { continue; }

        match decide(&hist, &cfg) {
            Sig::Hold => {}
            Sig::Buy  => trade("buy", &cfg, &redis, &db, user_id, is_demo, &master_key, &settings).await,
            Sig::Sell => trade("sell", &cfg, &redis, &db, user_id, is_demo, &master_key, &settings).await,
        }

        // cache last 400 bars in Redis (optional, useful for other services)
        let _ = redis.set_json("candles:BTCUSDT:4h", &hist, 3600 * 48).await;
    }
}

/// --------------------------------------------------------------------
async fn trade(
    side: &str,
    cfg: &MeanRevParams,
    redis:&RedisPool,
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
        log::error!("mean-reversion {side} err: {e:?}");
    }
}