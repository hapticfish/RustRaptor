// src/services/blowfin/ws.rs

use crate::config::settings::Settings;
use crate::services::blowfin::auth;
use crate::utils::errors::ApiError;
use tokio_tungstenite::connect_async;
use futures_util::{SinkExt, StreamExt};
use tungstenite::protocol::Message;
use serde_json::json;

/// Connects and authenticates to the private WS channel
pub async fn connect_private(settings: &Settings) -> Result<(), ApiError> {
    let url = if settings.is_demo() {
        "wss://demo-trading-openapi.blofin.com/ws/private"
    } else {
        "wss://openapi.blofin.com/ws/private"
    };
    let (mut ws, _) = connect_async(url).await?;

    let ts = auth::current_timestamp();
    let nonce = auth::generate_nonce();
    let sign = auth::sign_ws(&settings.blowfin_api_secret, &ts, &nonce);

    let login = json!({
        "op": "login",
        "args": [{
            "apiKey": settings.blowfin_api_key,
            "passphrase": settings.blowfin_api_passphrase,
            "timestamp": ts,
            "sign": sign,
            "nonce": nonce
        }]
    })
        .to_string();

    ws.send(Message::Text(login.into())).await?;

    while let Some(msg) = ws.next().await {
        let msg = msg?;
        println!("WS >> {:?}", msg);
    }
    Ok(())
}
