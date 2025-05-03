// src/services/trading_engine.rs

use crate::config::settings::Settings;
use crate::services::blowfin::api::{OrderRequest, BlowFinResponse};
use crate::utils::errors::{TradeError, ApiError};
use serde_json::Value;

#[derive(Debug, Clone)]
pub enum Exchange {
    Blowfin,
    // Future: Binance, Coinbase, etc.
}

#[derive(Debug)]
pub struct TradeRequest {
    pub exchange: Exchange,
    pub symbol: String,
    pub side: String,
    pub order_type: String,
    pub price: Option<f64>,
    pub size: f64,
}

#[derive(Debug, serde::Serialize)]
pub struct TradeResponse {
    pub success: bool,
    pub data: Value,
}

/// Dispatches the trade to the right exchange client
pub async fn execute_trade(
    req: TradeRequest,
    settings: &Settings,
) -> Result<TradeResponse, TradeError> {
    match req.exchange {
        Exchange::Blowfin => {
            let order_req = OrderRequest {
                inst_id: req.symbol.clone(),
                margin_mode: "isolated".into(),
                side: req.side.clone(),
                order_type: req.order_type.clone(),
                price: req.price.map(|p| p.to_string()),
                size: req.size.to_string(),
            };
            let resp: BlowFinResponse =
                crate::services::blowfin::api::place_order(settings, &order_req)
                    .await
                    .map_err(TradeError::Api)?;
            Ok(TradeResponse {
                success: resp.code == "0",
                data: resp.data,
            })
        }
    }
}
