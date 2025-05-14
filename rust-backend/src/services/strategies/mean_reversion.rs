// src/services/strategies/mean_reversion.rs

use crate::{
    config::settings::Settings,
    db::redis::RedisPool,
    services::trading_engine::{execute_trade, Exchange, TradeRequest},
};
use bigdecimal::ToPrimitive;
use chrono::{DateTime, Utc};
use redis::RedisError;
use serde::{Deserialize, Serialize};
use crate::utils::errors::TradeError;

/// ---------- parameters persisted in `user_strategies.params` ---------
#[derive(Clone, Deserialize)]
pub struct MeanRevParams {
    pub symbol: String,
    #[serde(default = "def_period")] pub period: usize,
    #[serde(default = "def_sigma")]  pub sigma:  f64,
    #[serde(default = "def_size")]   pub size:   f64,
}
fn def_period() -> usize { 20 }
fn def_sigma()  -> f64   { 2.0 }
fn def_size()   -> f64   { 0.01 }

/// ---------------------------- candle ---------------------------------
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Candle {
    pub open:  f64,
    pub high:  f64,
    pub low:   f64,
    pub close: f64,
    pub volume: f64,
    pub timestamp: DateTime<Utc>,
}

/// ----------------------- SMA / σ helpers -----------------------------
pub fn bollinger(c: &[Candle], period: usize, k: f64) -> Option<(f64,f64,f64)> {
    if c.len() < period { return None }
    let slice = &c[c.len()-period..];
    let sma   = slice.iter().map(|x| x.close).sum::<f64>() / period as f64;
    let var   = slice.iter().map(|x| (x.close-sma).powi(2)).sum::<f64>() / period as f64;
    let sd    = var.sqrt();
    Some((sma, sma+k*sd, sma-k*sd))
}

#[derive(Debug)] pub enum Signal { Buy, Sell, Hold }

fn signal(candles:&[Candle], cfg:&MeanRevParams) -> Signal {
    match bollinger(candles, cfg.period, cfg.sigma) {
        Some((mid, _up, low)) => {
            let p = candles.last().unwrap().close;
            if p < low      { Signal::Buy  }
            else if p>=mid  { Signal::Sell }
            else            { Signal::Hold }
        }
        None => Signal::Hold
    }
}

/// --------------------- Redis candle cache ----------------------------
async fn candles_from_cache(r:&RedisPool, sym:&str)->Result<Vec<Candle>,RedisError>{
    let k = format!("candles:{sym}:4h");
    Ok(r.get_json(&k).await?.unwrap_or_default())
}

/// ------------------------- trader ------------------------------------
async fn act(
    sig: Signal, cfg:&MeanRevParams, settings:&Settings
) -> Result<(), TradeError>{
    let side = match sig { Signal::Buy=>"buy", Signal::Sell=>"sell", _=>"hold" };
    if side=="hold" { return Ok(()) }

    execute_trade(
        TradeRequest{
            exchange: Exchange::Blowfin,
            symbol:   cfg.symbol.clone(),
            side:     side.into(),
            order_type:"market".into(),
            price: None,
            size:  cfg.size,
        },
        settings
    ).await.map(|_|())
}

/// one loop, used by `loop_forever`
async fn one_iteration(
    cfg:&MeanRevParams, redis:&RedisPool, settings:&Settings
){
    match candles_from_cache(redis, &cfg.symbol).await {
        Ok(candles)=>{
            let s = signal(&candles,cfg);
            if let Err(e)= act(s,cfg,settings).await{
                log::warn!("mean-rev act error: {e:?}");
            }
        }
        Err(e)=>log::warn!("redis candle fetch err: {e:?}")
    }
}

/// =============  Tokio task  ==========================================
pub async fn loop_forever(
    row: crate::services::scheduler::StrategyRow,
    redis: RedisPool,
    settings: Settings,
){
    let params: MeanRevParams = serde_json::from_value(row.params)
        .unwrap_or_else(|e| { log::error!("bad params: {e}"); std::process::exit(1) });

    let mut iv = tokio::time::interval(std::time::Duration::from_secs(4*3600));
    loop {
        one_iteration(&params,&redis,&settings).await;
        iv.tick().await;
    }
}