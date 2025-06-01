//! Centralised market‑data fan‑out for **all** real‑time strategies.
//! -----------------------------------------------------------------
//! ‣ Keeps WebSocket code in *one* place (separation of concerns).
//! ‣ Publishes `Candle` & `OrderBookSnapshot` streams via `tokio::broadcast`.
//! ‣ Agnostic to exchange – add new connectors behind `spawn_*_feed()`.
//!
//! Usage from a strategy task:
//! ```ignore
//! let mut rx_4h = bus.candles_4h.subscribe();
//! while let Ok(candle) = rx_4h.recv().await { /* feed engine */ }
//! ```
//! -----------------------------------------------------------------

use std::sync::Arc;
use tokio::sync::broadcast::{self, Sender};
// use tokio_stream::wrappers::BroadcastStream;
use chrono::{DateTime, Utc};
use futures_util::StreamExt;
use serde::Deserialize;
// use rust_decimal::Decimal;

use crate::services::strategies::{Candle, OrderBookSnapshot};
use crate::utils::signature::verify_hmac_bytes;

const CAPACITY: usize = 256; // ring‑buffer per topic

#[derive(Clone)]
pub struct MarketBus {
    pub candles_1h: Sender<Candle>,
    pub candles_4h: Sender<Candle>,
    pub order_book: Sender<OrderBookSnapshot>,
}

impl MarketBus {
    pub fn new() -> Self {
        let (c1h, _) = broadcast::channel(CAPACITY);
        let (c4h, _) = broadcast::channel(CAPACITY);
        let (ob, _) = broadcast::channel(CAPACITY);
        Self {
            candles_1h: c1h,
            candles_4h: c4h,
            order_book: ob,
        }
    }
}

impl Default for MarketBus {
    fn default() -> Self {
        Self::new()
    }
}

/* ─────────────────────────────────────────  Security toggle ────── */

/// Per-feed verification – default is `None` (no signature required)
#[derive(Clone)]
#[allow(dead_code)]
enum FeedSecurity {
    None,
    /// HMAC SHA-256 (hex) in a header / JSON field
    ///
    /// * `header` – HTTP header **or** JSON key that carries the hex digest
    /// * `secret_env` – env-var that holds the shared secret
    Hmac {
        header: &'static str,
        secret_env: &'static str,
    },
}

/// Checks `text` against `FeedSecurity`.
/// Returns `true` = accept frame, `false` = drop silently + `warn!`.
fn frame_ok(sec: &FeedSecurity, headers_or_json: &str, body: &[u8]) -> bool {
    match sec {
        FeedSecurity::None => true,
        FeedSecurity::Hmac { header, secret_env } => {
            // 1) extract sig – either from JSON or pretend headers string
            let sig = if headers_or_json.starts_with('{') {
                // very small fast-path parse; real impl can use serde_json::Value
                let key = format!(r#""{}":"#, header);
                headers_or_json
                    .split(&key)
                    .nth(1)
                    .and_then(|s| s.split('"').nth(1))
            } else {
                // header style: HeaderName: value\r\n
                headers_or_json
                    .lines()
                    .find(|l| {
                        l.to_ascii_lowercase()
                            .starts_with(&header.to_ascii_lowercase())
                    })
                    .and_then(|l| l.split(':').nth(1))
                    .map(str::trim)
            };

            if let Some(sig_hex) = sig {
                let secret = std::env::var(secret_env).unwrap_or_default();
                if verify_hmac_bytes(body, &secret, sig_hex) {
                    true
                } else {
                    log::warn!("feed frame failed HMAC check ({header})");
                    false
                }
            } else {
                log::warn!("feed frame missing signature header/field ({header})");
                false
            }
        }
    }
}

// ================================================================
// Exchange connectors – each spawns its own task & forwards to bus
// ================================================================

pub async fn spawn_all_feeds(settings: &crate::config::settings::Settings) -> Arc<MarketBus> {
    let bus = Arc::new(MarketBus::new());

    // Binance – unsigned public stream
    tokio::spawn(binance_feed(Arc::clone(&bus), FeedSecurity::None));

    // BlowFin private depth feed – also unsigned
    tokio::spawn(blowfin_depth_feed(
        settings.clone(),
        Arc::clone(&bus),
        FeedSecurity::None,
    ));

    bus
}

/* ─────────────────────────────────────────  Binance WS ────── */

async fn binance_feed(bus: Arc<MarketBus>, sec: FeedSecurity) {
    use tokio_tungstenite::connect_async;
    use tungstenite::Message;

    let url = "wss://stream.binance.com:9443/stream?streams=btcusdt@kline_1h/btcusdt@kline_4h";
    let (mut ws, _) = match connect_async(url).await {
        Ok(t) => t,
        Err(e) => {
            log::error!("binance ws connect: {e}");
            return;
        }
    };

    while let Some(Ok(msg)) = ws.next().await {
        if let Message::Text(txt) = &msg {
            if !frame_ok(&sec, txt, txt.as_bytes()) {
                continue;
            }

            if let Ok(ev) = serde_json::from_str::<BinanceStreamEvent>(txt) {
                if let Some(k) = ev.data.kline {
                    let candle = Candle {
                        ts: DateTime::<Utc>::from_timestamp_millis(k.close_time as i64).unwrap(),
                        open: k.open(),
                        high: k.high(),
                        low: k.low(),
                        close: k.close(),
                        volume: k.volume(),
                        delta: None,
                    };
                    match k.interval.as_str() {
                        "1h" => {
                            let _ = bus.candles_1h.send(candle);
                        }
                        "4h" => {
                            let _ = bus.candles_4h.send(candle);
                        }
                        _ => {}
                    };
                }
            }
        }
    }
}

/* ─────────────────────────────────────────  Binance structs ─ */

#[derive(Debug, Deserialize)]
struct BinanceStreamEvent {
    #[allow(dead_code)]
    stream: String,
    data: BinanceKlineWrapper,
}

#[derive(Debug, Deserialize)]
struct BinanceKlineWrapper {
    #[serde(rename = "k")]
    kline: Option<BinanceKline>,
}

#[derive(Debug, Deserialize)]
struct BinanceKline {
    #[serde(rename = "T")]
    close_time: u64,
    #[serde(rename = "i")]
    interval: String,
    #[serde(rename = "o")]
    open: String,
    #[serde(rename = "h")]
    high: String,
    #[serde(rename = "l")]
    low: String,
    #[serde(rename = "c")]
    close: String,
    #[serde(rename = "v")]
    volume: String,
}

impl BinanceKline {
    fn parse_f64(s: &str) -> f64 {
        s.parse::<f64>().unwrap_or(0.0)
    }
    fn open(&self) -> f64 {
        Self::parse_f64(&self.open)
    }
    fn high(&self) -> f64 {
        Self::parse_f64(&self.high)
    }
    fn low(&self) -> f64 {
        Self::parse_f64(&self.low)
    }
    fn close(&self) -> f64 {
        Self::parse_f64(&self.close)
    }
    fn volume(&self) -> f64 {
        Self::parse_f64(&self.volume)
    }
}

// ───────────────────────────────────────── BlowFin private depth fan-out ────
async fn blowfin_depth_feed(
    settings: crate::config::settings::Settings,
    bus: Arc<MarketBus>,
    sec: FeedSecurity,
) {
    use crate::services::blowfin::ws::{connect_private, DepthFrame};
    use tokio::sync::mpsc;

    // channel between WS task ↔ market_data task
    let (tx, mut rx) = mpsc::channel::<DepthFrame>(64);

    // ❶ spawn WS handler
    tokio::spawn(async move {
        if let Err(e) = connect_private(&settings, tx).await {
            log::error!("blowfin private ws exit: {e}");
        }
    });

    // ❷ forward verified frames onto MarketBus
    while let Some(df) = rx.recv().await {
        if !frame_ok(&sec, "", &df.raw_bytes) {
            log::warn!("blowfin depth: bad sig – dropped");
            continue;
        }
        let snap = OrderBookSnapshot {
            bid_depth: df.bid_sum,
            ask_depth: df.ask_sum,
        };
        let _ = bus.order_book.send(snap);
    }
}
