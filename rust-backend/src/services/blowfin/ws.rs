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


