// src/routes/trading.rs

use crate::config::settings::Settings;
use crate::middleware::path_logger::PathLogger;
use crate::services::blowfin::api::get_balance;
use crate::services::trading_engine::{execute_trade, Exchange, TradeRequest, TradeResponse};
use crate::utils::types::ApiResponse;
use actix_web::dev::HttpServiceFactory;
use actix_web::{get, post, web, HttpMessage, HttpResponse, Responder};
use serde::Deserialize;
use serde_json::Value;

#[derive(Debug, Deserialize)]
pub struct TradeParams {
    pub exchange: String,
    pub symbol: String,
    pub side: String,
    pub order_type: String,
    pub price: Option<f64>,
    pub size: f64,
}

#[post("/trade")]
pub async fn trade(
    params: web::Json<TradeParams>,
    settings: web::Data<Settings>,
    db: web::Data<sqlx::PgPool>,
    req: actix_web::HttpRequest,
) -> impl Responder {
    let exchange = match params.exchange.to_lowercase().as_str() {
        "blowfin" => Exchange::Blowfin,
        _ => {
            return HttpResponse::BadRequest().json(ApiResponse::<()> {
                success: false,
                message: Some("Unsupported exchange".to_string()),
                data: None,
            })
        }
    };

    // --- Extract user_id from JWT extension (auth middleware puts it there) ---
    let user_id: i64 = req
        .extensions()
        .get::<String>()
        .and_then(|uid_str| uid_str.parse::<i64>().ok())
        .unwrap_or(0); // You may want to error if missing

    // -- Demo flag, could also be per-user (here: from settings) --
    let is_demo = settings.is_demo();

    // -- Your master key for decryption (from ENV or secret management) --
    let master_key = std::env::var("MASTER_KEY").unwrap_or_default();
    let master_key_bytes = master_key.as_bytes();

    let req_struct = TradeRequest {
        exchange,
        symbol: params.symbol.clone(),
        side: params.side.clone(),
        order_type: params.order_type.clone(),
        price: params.price,
        size: params.size,
    };

    match execute_trade(req_struct, db.as_ref(), user_id, is_demo, master_key_bytes).await {
        Ok(resp) => HttpResponse::Ok().json(ApiResponse::<TradeResponse> {
            success: true,
            message: Some("Trade executed successfully".to_string()),
            data: Some(resp),
        }),
        Err(e) => HttpResponse::InternalServerError().json(ApiResponse::<()> {
            success: false,
            message: Some(format!("Trade error: {}", e)),
            data: None,
        }),
    }
}

#[get("/balance")]
pub async fn balance(
    settings: web::Data<Settings>,
    db: web::Data<sqlx::PgPool>,
    req: actix_web::HttpRequest,
) -> impl Responder {
    let user_id: i64 = req
        .extensions()
        .get::<String>()
        .and_then(|uid_str| uid_str.parse::<i64>().ok())
        .unwrap_or(0);

    let is_demo = settings.is_demo();
    let master_key = std::env::var("MASTER_KEY").unwrap_or_default();
    let master_key_bytes = master_key.as_bytes();

    match get_balance(db.as_ref(), user_id, is_demo, master_key_bytes).await {
        Ok(resp) => HttpResponse::Ok().json(ApiResponse::<Value> {
            success: true,
            message: Some("Balance fetched successfully".to_string()),
            data: Some(resp.data),
        }),
        Err(e) => HttpResponse::InternalServerError().json(ApiResponse::<()> {
            success: false,
            message: Some(format!("Balance error: {}", e)),
            data: None,
        }),
    }
}

#[get("/test")]
pub async fn test_trade_api() -> impl Responder {
    HttpResponse::Ok().body("Trading scope is active.")
}

#[get("/routes")]
pub async fn list_routes() -> impl Responder {
    let routes = vec!["/health", "/api/trade", "/api/balance", "/api/test"];

    HttpResponse::Ok().json(routes)
}

#[get("/simple")]
pub async fn simple_test() -> impl Responder {
    HttpResponse::Ok().body("Simple test route")
}

pub fn trading_scope() -> impl HttpServiceFactory {
    web::scope("/api")
        .wrap(PathLogger)
        .service(simple_test)
        .service(test_trade_api)
        .service(balance)
        .service(trade)
        .service(list_routes)
}
