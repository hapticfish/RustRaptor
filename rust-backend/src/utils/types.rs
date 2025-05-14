use serde::{Deserialize, Serialize};
use sqlx::{Type, postgres::PgTypeInfo};


#[derive(Debug, Serialize)]
pub struct ApiResponse<T> {
    pub success: bool,
    pub message: Option<String>,
    pub data: Option<T>,
}

/* ------------------------- Postgres ENUMs ------------------------ */

#[derive(Debug, Serialize, Deserialize, Type)]
#[sqlx(type_name = "market_type_enum", rename_all = "lowercase")]
pub enum MarketType { Spot, Futures, Swap, Options }



#[derive(Debug, Serialize, Deserialize, Type)]
#[sqlx(type_name = "order_type_enum", rename_all = "lowercase")]
pub enum OrderType { Market, Limit, PostOnly, Fok, Ioc, Trigger, Conditional }



#[derive(Debug, Serialize, Deserialize, Type)]
#[sqlx(type_name = "order_status", rename_all = "lowercase")]
pub enum OrderStatus { Live, PartiallyFilled, Filled, Cancelled, Rejected }



#[derive(Debug, Serialize, Deserialize, Type)]
#[sqlx(type_name = "maker_taker_enum", rename_all = "lowercase")]
pub enum MakerTaker { Maker, Taker }



#[derive(Debug, Serialize, Deserialize, Type)]
#[sqlx(type_name = "fee_type_enum", rename_all = "lowercase")]
pub enum FeeType { Maker, Taker, Funding, Rebate }

impl<T: serde::Serialize> ApiResponse<T> {
    pub fn ok(data: T) -> Self {
        Self { success: true, message: None, data: Some(data) }
    }
    pub fn err(msg: &str) -> Self {
        Self { success: false, message: Some(msg.into()), data: None }
    }
}
