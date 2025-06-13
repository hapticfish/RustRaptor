// src/services/strategies/common.rs
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Candle {
    pub ts: DateTime<Utc>,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
    /// Optional cumulative delta
    pub delta: Option<f64>,
}

// ----------------------------------- basic candle helpers -------------
impl Default for Candle {
    fn default() -> Self {
        Candle {
            ts: Default::default(),
            open: 0.0,
            high: 0.0,
            low: 0.0,
            close: 0.0,
            volume: 0.0,
            delta: None,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct OrderBookSnapshot {
    pub bid_depth: f64,
    pub ask_depth: f64,
}
