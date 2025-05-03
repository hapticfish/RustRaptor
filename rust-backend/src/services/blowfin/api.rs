// src/services/blowfin/api.rs

use crate::config::settings::Settings;
use crate::services::blowfin::auth;
use crate::utils::errors::ApiError;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Serialize)]
pub struct OrderRequest {
    #[serde(rename = "instId")]
    pub inst_id: String,
    #[serde(rename = "marginMode")]
    pub margin_mode: String,
    pub side: String,
    #[serde(rename = "orderType")]
    pub order_type: String,
    pub price: Option<String>,
    pub size: String,
}

#[derive(Debug, Deserialize)]
pub struct BlowFinResponse {
    pub code: String,
    pub msg: String,
    pub data: Value,
}

/// Place an order with BlowFin
pub async fn place_order(
    settings: &Settings,
    order: &OrderRequest,
) -> Result<BlowFinResponse, ApiError> {
    let path = "/api/v1/trade/order";
    let base = if settings.is_demo() {
        "https://demo-trading-openapi.blofin.com"
    } else {
        "https://openapi.blofin.com"
    };
    let url = format!("{}{}", base, path);

    let ts = auth::current_timestamp();
    let nonce = auth::generate_nonce();
    let body = serde_json::to_string(order)?;
    let sign = auth::sign_rest(&settings.blowfin_api_secret, "POST", path, &ts, &nonce, &body);

    let client = Client::new();
    let resp = client
        .post(&url)
        .header("ACCESS-KEY", &settings.blowfin_api_key)
        .header("ACCESS-SIGN", sign)
        .header("ACCESS-TIMESTAMP", ts)
        .header("ACCESS-NONCE", nonce)
        .header("ACCESS-PASSPHRASE", &settings.blowfin_api_passphrase)
        .json(order)
        .send()
        .await?
        .json::<BlowFinResponse>()
        .await?;
    Ok(resp)
}

/// Fetch account balances
pub async fn get_balance(
    settings: &Settings,
) -> Result<BlowFinResponse, ApiError> {
    let path = "/api/v1/asset/balances?accountType=futures";
    let base = if settings.is_demo() {
        "https://demo-trading-openapi.blofin.com"
    } else {
        "https://openapi.blofin.com"
    };
    let url = format!("{}{}", base, path);

    let ts = auth::current_timestamp();
    let nonce = auth::generate_nonce();
    let sign = auth::sign_rest(&settings.blowfin_api_secret, "GET", path, &ts, &nonce, "");

    let client = Client::new();
    let resp = client
        .get(&url)
        .header("ACCESS-KEY", &settings.blowfin_api_key)
        .header("ACCESS-SIGN", sign)
        .header("ACCESS-TIMESTAMP", ts)
        .header("ACCESS-NONCE", nonce)
        .header("ACCESS-PASSPHRASE", &settings.blowfin_api_passphrase)
        .send()
        .await?
        .json::<BlowFinResponse>()
        .await?;
    Ok(resp)
}
