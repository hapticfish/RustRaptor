// src/services/strategies/mean_reversion.rs

use crate::{
    config::settings::Settings,
    db::redis::RedisPool,
    services::trading_engine::{execute_trade, Exchange, TradeError, TradeRequest},
};
use bigdecimal::ToPrimitive;
use chrono::{DateTime, Utc};
use redis::RedisError;
use serde::{Deserialize, Serialize};

/// How many candles to use & how many σ for the bands
#[derive(Debug, Clone)]
pub struct BollingerConfig {
    pub period: usize,
    pub std_dev_factor: f64,
}

/// OHLCV candle shape (4h)
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Candle {
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug)]
pub enum Signal {
    Buy,
    Sell,
    Hold,
}

/// Calculate mid, upper, lower bands. Returns None if insufficient data.
pub fn calculate_bollinger(
    candles: &[Candle],
    cfg: &BollingerConfig,
) -> Option<(f64, f64, f64)> {
    if candles.len() < cfg.period {
        return None;
    }

    let slice = &candles[candles.len() - cfg.period..];
    let closes: Vec<f64> = slice.iter().map(|c| c.close).collect();
    let sma = closes.iter().sum::<f64>() / cfg.period as f64;
    let var = closes
        .iter()
        .map(|&p| (p - sma).powi(2))
        .sum::<f64>()
        / cfg.period as f64;
    let std = var.sqrt();

    let upper = sma + cfg.std_dev_factor * std;
    let lower = sma - cfg.std_dev_factor * std;
    Some((sma, upper, lower))
}

/// Turn bands + latest price into a discrete signal
pub fn generate_signal(candles: &[Candle], cfg: &BollingerConfig) -> Signal {
    if let Some((mid, _upper, lower)) = calculate_bollinger(candles, cfg) {
        let price = candles.last().unwrap().close;
        if price < lower {
            Signal::Buy
        } else if price >= mid {
            Signal::Sell
        } else {
            Signal::Hold
        }
    } else {
        Signal::Hold
    }
}

/// Fetch cached candles from Redis and deserialize
async fn fetch_cached_candles(
    redis: &RedisPool,
    symbol: &str,
) -> Result<Vec<Candle>, RedisError> {
    let key = format!("candles:{symbol}:4h");
    if let Some(v) = redis.get_json(&key).await? {
        Ok(v)
    } else {
        Ok(vec![])
    }
}

/// Given a signal, fire a market trade for a fixed size
async fn act_on_signal(
    signal: Signal,
    settings: &Settings,
    size: f64,
) -> Result<(), TradeError> {
    if let Signal::Hold = signal {
        return Ok(());
    }

    let side = match signal {
        Signal::Buy => "buy",
        Signal::Sell => "sell",
        _ => unreachable!(),
    };

    let req = TradeRequest {
        exchange: Exchange::Blowfin,
        symbol: settings.default_strategy.clone(), // or pass symbol
        side: side.into(),
        order_type: "market".into(),
        price: None,
        size,
    };

    execute_trade(req, settings).await.map(|_| ())
}

/// Entry point, invoked on your chosen schedule
pub async fn run_mean_reversion(
    redis: RedisPool,
    settings: Settings,
) {
    let cfg = BollingerConfig {
        period: 20,
        std_dev_factor: 2.0,
    };

    // 1. Fetch candles
    let candles = match fetch_cached_candles(&redis, &settings.default_strategy).await {
        Ok(c) => c,
        Err(e) => {
            log::error!("Failed to fetch candles from Redis: {}", e);
            return;
        }
    };

    // 2. Generate signal
    let signal = generate_signal(&candles, &cfg);
    log::info!("MeanReversion signal: {:?}", signal);

    // 3. Act (0.01 size hard-coded for now)
    if let Err(e) = act_on_signal(signal, &settings, 0.01).await {
        log::error!("MeanReversion trade failed: {:?}", e);
    }
}
