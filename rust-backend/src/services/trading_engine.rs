// src/services/trading_engine.rs

use crate::services::blowfin::api::{OrderRequest};
use crate::services::risk;

pub(crate) use crate::utils::errors::{TradeError};
use serde_json::Value;
use sqlx::PgPool;

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

#[derive(Debug, Clone, serde::Serialize)]
pub struct TradeResponse {
    pub success: bool,
    pub exchange: Exchange,
    pub symbol: String,
    pub side: String,
    pub order_type: String,
    pub price: Option<f64>,
    pub size: f64,
    pub data: Value,
}

pub async fn execute_trade(
    req: TradeRequest,
    db: &PgPool,
    user_id: i64,
    is_demo: bool,
    master_key: &[u8],
) -> Result<TradeResponse, TradeError> {

    risk::check_slippage(0.0)?;


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

            let resp = crate::services::blowfin::api::place_order(
                db,
                user_id,
                &order_req,
                is_demo,
                master_key
            )
                .await
                .map_err(TradeError::Api)?;

            Ok(TradeResponse {
                success: resp.code == "0",
                exchange: req.exchange.clone(),
                symbol: req.symbol,
                side: req.side,
                order_type: req.order_type,
                price: req.price,
                size: req.size,
                data: resp.data,
            })
        }
    }
}