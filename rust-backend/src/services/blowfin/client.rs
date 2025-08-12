//! Production adapter that talks to BlowFin’s REST API.
//! Implements the `ApiClient` trait expected by `trading_engine.rs`.

use crate::db::api_keys::DecryptedApiKey;
use crate::services::blowfin::api::OrderRequest;
use crate::utils::errors::TradeError;
use crate::utils::types::OrderResp;                // make sure this struct exists
use crate::services::trading_engine::{ApiClient, ApiResponse};
use async_trait::async_trait;
use reqwest::{Client, StatusCode};
use sqlx::PgPool;
use serde_json::Value;

pub struct BlowfinClient {
    http:  Client,
    creds: DecryptedApiKey,
}

impl BlowfinClient {
    /// Factory – you’ll usually call this inside `execute_trade`.
    pub async fn new(creds: DecryptedApiKey) -> Self {
        Self { http: Client::new(), creds }
    }

    /// Low-level helper used only inside the trait impl below.
    async fn signed_post(
        &self,
        endpoint: &str,
        body: &Value,
    ) -> Result<OrderResp, TradeError> {
        // TODO: real HMAC with self.creds.api_secret
        let resp = self
            .http
            .post(format!("https://api.blowfin.com{endpoint}"))
            .json(body)
            .send()
            .await
            .map_err(|e| TradeError::Api(e.into()))?;

        if resp.status() != StatusCode::OK {
            return Err(TradeError::Api(format!("http {}", resp.status()).into()));
        }

        Ok(resp.json::<OrderResp>().await.map_err(|e| TradeError::Api(e.into()))?)
    }
}



#[async_trait]
impl ApiClient for BlowfinClient {
    async fn place_order(
        &self,
        _db: &PgPool,
        _user_id: i64,
        order: &OrderRequest,
        _is_demo: bool,
        _master_key: &[u8],
    ) -> Result<ApiResponse, TradeError> {
        let payload = serde_json::to_value(order).expect("serialise order");
        let raw     = self.signed_post("/v1/order", &payload).await?;

        Ok(ApiResponse {
            code: raw.status.clone(),
            data: serde_json::to_value(raw).expect("serialise OrderResp"),
        })
    }
}
