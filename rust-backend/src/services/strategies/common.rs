// src/services/strategies/common.rs
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Candle {
    pub ts:     DateTime<Utc>,
    pub open:   f64,
    pub high:   f64,
    pub low:    f64,
    pub close:  f64,
    pub volume: f64,
    /// Optional cumulative delta
    pub delta:  Option<f64>,
}

#[derive(Debug, Clone, Copy)]
pub struct OrderBookSnapshot {
    pub bid_depth: f64,
    pub ask_depth: f64,
}
