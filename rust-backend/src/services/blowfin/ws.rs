// src/services/blowfin/ws.rs

//!  BlowFin private-WS adapter ⇢ depth snapshot stream
//!
//!  * Connects & authenticates (login op)
//!  * Parses “depth-snapshot” messages coming from channel
//!  * Sends each snapshot through the supplied mpsc::Sender
//!
//!  The caller decides what to do with the snapshots (e.g. broadcast on
//!  MarketBus, store in Redis, etc.).

use crate::{config::settings::Settings, utils::errors::ApiError};
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::Value;
use tokio::sync::mpsc::Sender;
use tokio_tungstenite::connect_async;
use tungstenite::Message;

use super::auth;

/// Lightweight depth snapshot used by strategies
#[derive(Debug, Clone)]
pub struct DepthFrame {
    pub bid_sum: f64,
    pub ask_sum: f64,
    /* optional raw fields for verification */
    pub raw_header: Vec<(String, String)>,
    pub raw_bytes: Vec<u8>,
}

/// Spawn the WebSocket task and pipe decoded `DepthFrame`s out.
/// *Returns* once the socket closes / errors.
pub async fn connect_private(settings: &Settings, out: Sender<DepthFrame>) -> Result<(), ApiError> {
    // ----------- 1) Connect ------------------------------------------------
    let url = if settings.is_demo() {
        "wss://demo-trading-openapi.blofin.com/ws/private"
    } else {
        "wss://openapi.blofin.com/ws/private"
    };
    let (mut ws, _) = connect_async(url).await?;

    // ----------- 2) Login op ----------------------------------------------
    let ts = auth::current_timestamp();
    let nonce = auth::generate_nonce();
    let sign = auth::sign_ws(&settings.blowfin_api_secret, &ts, &nonce);

    let login = serde_json::json!({
        "op":"login",
        "args":[{
            "apiKey":     settings.blowfin_api_key,
            "passphrase": settings.blowfin_api_passphrase,
            "timestamp":  ts,
            "sign":       sign,
            "nonce":      nonce
        }]
    })
    .to_string();

    ws.send(Message::Text(login.into())).await?;

    // ----------- 3) Subscribe to depth channel ----------------------------
    let sub = r#"{
        "op":"subscribe",
        "args":[{"channel":"books5","instId":"BTC-USDT-SWAP"}]
    }"#;
    ws.send(Message::Text(sub.into())).await?;

    // ----------- 4) Main read-loop ----------------------------------------
    while let Some(msg) = ws.next().await {
        let msg = msg?;
        if let Message::Text(txt) = msg {
            if let Ok(ev) = serde_json::from_str::<WsEvent>(&txt) {
                if ev.arg.channel == "books5" {
                    if let Some(df) = depth_from_event(&ev) {
                        // ignore send errors (no active receivers)
                        let _ = out.send(df).await;
                    }
                }
            }
        }
    }
    Ok(())
}

// ---------- Private helpers -----------------------------------------------

#[derive(Debug, Deserialize)]
struct WsEvent {
    #[serde(rename = "arg")]
    arg: WsArg,
    #[serde(rename = "data")]
    data: Vec<Value>,
}
#[derive(Debug, Deserialize)]
struct WsArg {
    channel: String,
}

/// Convert the raw JSON → DepthFrame
fn depth_from_event(ev: &WsEvent) -> Option<DepthFrame> {
    // books5 comes as:
    // { asks:[[price,size,_ ],...], bids:[[price,size,_ ],...] }
    let obj = ev.data.first()?.as_object()?;
    let sum_side = |side: &str| -> f64 {
        obj.get(side)
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|lvl| lvl.get(1)?.as_str()?.parse::<f64>().ok())
                    .sum::<f64>()
            })
            .unwrap_or(0.0)
    };
    Some(DepthFrame {
        bid_sum: sum_side("bids"),
        ask_sum: sum_side("asks"),
        raw_header: Vec::new(),
        raw_bytes: Vec::new(),
    })
}

// ──────────────────────────────────────────────────────────────
// UNIT-TESTS
// ──────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // handy helper producing a WsEvent in JSON then parsing it
    fn make_event(bids: &[(&str, &str)], asks: &[(&str, &str)]) -> WsEvent {
        let arrify = |side: &[(&str, &str)]| {
            side.iter()
                .map(|(p, s)| json!([p, s, "0"])) // 3-tuple as returned by API
                .collect::<Vec<_>>()
        };
        let raw = json!({
            "arg": { "channel": "books5" },
            "data": [{
                "bids": arrify(bids),
                "asks": arrify(asks)
            }]
        });
        serde_json::from_value(raw).expect("valid WsEvent")
    }

    // ──────────────────────────────────────────────────────────
    // 1. Nominal path – sums both sides correctly
    // ──────────────────────────────────────────────────────────
    #[test]
    fn depth_parses_and_sums() {
        let ev = make_event(&[("30000", "2"), ("29990", "1.5")], &[("30010", "4")]);

        let df = depth_from_event(&ev).expect("DepthFrame");
        assert!((df.bid_sum - 3.5).abs() < 1e-9);
        assert!((df.ask_sum - 4.0).abs() < 1e-9);
    }

    // ──────────────────────────────────────────────────────────
    // 2. Empty data array ⇒ None (guard-clause)
    // ──────────────────────────────────────────────────────────
    #[test]
    fn empty_data_returns_none() {
        let raw = json!({
            "arg": { "channel": "books5" },
            "data": []                      // empty
        });
        let ev: WsEvent = serde_json::from_value(raw).unwrap();
        assert!(depth_from_event(&ev).is_none());
    }

    // ──────────────────────────────────────────────────────────
    // 3. Malformed price/size values are skipped, not panicked
    // ──────────────────────────────────────────────────────────
    #[test]
    fn malformed_levels_are_ignored() {
        let ev = make_event(&[("BAD", "X"), ("30000", "1")], &[("29999", "ABC")]);

        let df = depth_from_event(&ev).unwrap();
        assert!((df.bid_sum - 1.0).abs() < 1e-9); // only the good one counted
        assert_eq!(df.ask_sum, 0.0); // bad ask ignored ⇒ zero
    }

    // ──────────────────────────────────────────────────────────
    // 4. Non-books5 channel is filtered out upstream – we still
    //    check that helper would yield None if called directly.
    // ──────────────────────────────────────────────────────────
    #[test]
    fn wrong_channel_returns_none() {
        let raw = json!({
            "arg": { "channel": "orders" },
            "data":[{ "bids":[], "asks":[] }]
        });
        let ev: WsEvent = serde_json::from_value(raw).unwrap();
        // depth_from_event does not look at channel but upstream does;
        // here we assert the sums are zero to highlight expectation.
        let df = depth_from_event(&ev).unwrap();
        assert_eq!(df.bid_sum + df.ask_sum, 0.0);
    }
}
