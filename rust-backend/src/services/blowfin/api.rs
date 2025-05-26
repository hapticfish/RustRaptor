use crate::utils::errors::ApiError;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use crate::db::api_keys::{ApiKey};
use sqlx::PgPool;

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

/// Place an order with BlowFin using per-user API key
pub async fn place_order(
    db: &PgPool,
    user_id: i64,
    order: &OrderRequest,
    is_demo: bool,
    master_key: &[u8], // Used for decrypting
) -> Result<BlowFinResponse, ApiError> {
    let path = "/api/v1/trade/order";
    let base = if is_demo {
        "https://demo-trading-openapi.blofin.com"
    } else {
        "https://openapi.blofin.com"
    };
    let url = format!("{}{}", base, path);

    // 1. Get user's API key from DB
    let api_key_row = ApiKey::get_by_user_and_exchange(db, user_id, "blowfin").await?
        .ok_or_else(|| ApiError::Custom("Missing API key for BlowFin".into()))?;

    // 2. Decrypt API key
    let creds = api_key_row.decrypt(master_key)
        .map_err(|e| ApiError::Custom(format!("decrypt failed: {e}")))?;

    // 3. Sign the request with the decrypted secret
    let ts = crate::services::blowfin::auth::current_timestamp();
    let nonce = crate::services::blowfin::auth::generate_nonce();
    let body = serde_json::to_string(order)?;
    let sign = crate::services::blowfin::auth::sign_rest(
        &creds.api_secret,
        "POST",
        path,
        &ts,
        &nonce,
        &body,
    );

    let client = Client::new();
    let resp = client
        .post(&url)
        .header("ACCESS-KEY", &creds.api_key)
        .header("ACCESS-SIGN", sign)
        .header("ACCESS-TIMESTAMP", ts)
        .header("ACCESS-NONCE", nonce)
        .header("ACCESS-PASSPHRASE", &creds.api_passphrase)
        .json(order)
        .send()
        .await?
        .json::<BlowFinResponse>()
        .await?;

    Ok(resp)
}

/// Fetch account balances for a user
pub async fn get_balance(
    db: &PgPool,
    user_id: i64,
    is_demo: bool,
    master_key: &[u8],
) -> Result<BlowFinResponse, ApiError> {
    let path = "/api/v1/asset/balances?accountType=futures";
    let base = if is_demo {
        "https://demo-trading-openapi.blofin.com"
    } else {
        "https://openapi.blofin.com"
    };
    let url = format!("{}{}", base, path);

    // 1. Get and decrypt API key
    let api_key_row = ApiKey::get_by_user_and_exchange(db, user_id, "blowfin").await?
        .ok_or_else(|| ApiError::Custom("Missing API key for BlowFin".into()))?;
    let creds = api_key_row.decrypt(master_key)
        .map_err(|e| ApiError::Custom(format!("decrypt failed: {e}")))?;

    // 2. Sign
    let ts = crate::services::blowfin::auth::current_timestamp();
    let nonce = crate::services::blowfin::auth::generate_nonce();
    let sign = crate::services::blowfin::auth::sign_rest(
        &creds.api_secret,
        "GET",
        path,
        &ts,
        &nonce,
        "",
    );

    let client = Client::new();
    let resp = client
        .get(&url)
        .header("ACCESS-KEY", &creds.api_key)
        .header("ACCESS-SIGN", sign)
        .header("ACCESS-TIMESTAMP", ts)
        .header("ACCESS-NONCE", nonce)
        .header("ACCESS-PASSPHRASE", &creds.api_passphrase)
        .send()
        .await?
        .json::<BlowFinResponse>()
        .await?;
    Ok(resp)
}