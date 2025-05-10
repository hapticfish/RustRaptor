// src/routes/trading.rs

use actix_web::{post, get, web, HttpResponse, Responder, Scope};
use serde::Deserialize;
use actix_web::dev::{HttpServiceFactory, ServiceFactory, ServiceRequest, ServiceResponse};
use actix_web::Error;
use crate::config::settings::Settings;
use crate::services::trading_engine::{execute_trade, Exchange, TradeRequest, TradeResponse};
use crate::services::blowfin::api::get_balance;
use crate::utils::types::ApiResponse;
use serde_json::Value;
use crate::middleware::path_logger::PathLogger;

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

    let req = TradeRequest {
        exchange,
        symbol: params.symbol.clone(),
        side: params.side.clone(),
        order_type: params.order_type.clone(),
        price: params.price,
        size: params.size,
    };

    match execute_trade(req, &settings).await {
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
) -> impl Responder {
    match get_balance(&settings).await {
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
    let routes = vec![
        "/health",
        "/api/trade",
        "/api/balance",
        "/api/test",
    ];

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
