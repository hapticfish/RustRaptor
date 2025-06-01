use crate::utils::types::*;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::types::BigDecimal;
use sqlx::FromRow;
use uuid::Uuid;

/* -------------------------- USERS -------------------------- */

#[derive(Debug, Serialize, Deserialize, FromRow)]
pub struct User {
    pub user_id: i64,
    pub rr_username: String,
    pub email: Option<String>,
    pub created_at: Option<DateTime<Utc>>,
}

/* ------------------------- API KEYS ------------------------ */

#[derive(Debug, Serialize, Deserialize, FromRow)]
pub struct ApiKey {
    pub key_id: Uuid,
    pub user_id: i64,
    pub exchange: String,
    pub encrypted_api_key: Vec<u8>,
    pub encrypted_secret: Vec<u8>,
    pub encrypted_passphrase: Option<Vec<u8>>,
    pub created_at: Option<DateTime<Utc>>,
}

/* -------------------- EXCHANGE ACCOUNTS -------------------- */

#[derive(Debug, Serialize, Deserialize, FromRow)]
pub struct ExchangeAccount {
    pub acct_id: Uuid,
    pub user_id: i64,
    pub exchange: String,
    pub label: Option<String>,
    pub leverage_default: Option<BigDecimal>,
    pub demo: Option<bool>,
}

/* ------------------------- STRATEGIES DEPRECATED ---------------------- */

#[derive(Debug, Serialize, Deserialize, FromRow)]
pub struct StrategyDeprecated {
    pub strategy_id: Uuid,
    pub user_id: i64,
    pub name: String,
    pub params: serde_json::Value,
    pub active: Option<bool>,
    pub updated_at: Option<DateTime<Utc>>,
}

/* ---------------------- COPY RELATIONS --------------------- */

#[derive(Debug, Serialize, Deserialize, FromRow)]
pub struct CopyRelation {
    pub relation_id: Uuid,
    pub leader_user_id: i64,
    pub follower_user_id: i64,
    pub since: Option<DateTime<Utc>>,
    pub until: Option<DateTime<Utc>>,
    pub status: Option<String>,
}

/* --------------------------- ORDERS ------------------------ */

#[derive(Debug, Serialize, Deserialize, FromRow)]
pub struct Order {
    pub order_id: Uuid,
    pub external_order_id: Option<String>,
    pub user_id: i64,
    pub exchange: String,
    pub market_type: MarketType,
    pub symbol: String,
    pub side: String,
    pub order_type: OrderType,
    pub price: Option<BigDecimal>,
    pub size: BigDecimal,
    pub reduce_only: Option<bool>,
    pub margin_mode: Option<String>,
    pub position_side: Option<String>,
    pub status: OrderStatus,
    pub opened_at: Option<DateTime<Utc>>,
    pub closed_at: Option<DateTime<Utc>>,
}

/* --------------------------- FILLS ------------------------- */

#[derive(Debug, Serialize, Deserialize, FromRow)]
pub struct Fill {
    pub fill_id: Uuid,
    pub order_id: Uuid,
    pub maker_taker: MakerTaker,
    pub fill_price: BigDecimal,
    pub fill_size: BigDecimal,
    pub trade_fee: Option<BigDecimal>,
    pub funding_fee: Option<BigDecimal>,
    pub realised_pnl: Option<BigDecimal>,
    pub executed_at: DateTime<Utc>,
}

/* ---------------------------- FEES ------------------------- */

#[derive(Debug, Serialize, Deserialize, FromRow)]
pub struct Fee {
    pub fee_id: Uuid,
    pub user_id: i64,
    pub exchange: String,
    pub symbol: Option<String>,
    pub fee_type: FeeType,
    pub amount: BigDecimal,
    pub reference_id: Option<Uuid>,
    pub occurred_at: DateTime<Utc>,
}

/* -------------------------- POSITIONS ---------------------- */

#[derive(Debug, Serialize, Deserialize, FromRow)]
pub struct Position {
    pub snapshot_id: Uuid,
    pub user_id: i64,
    pub exchange: String,
    pub symbol: String,
    pub market_type: MarketType,
    pub side: String,
    pub size: BigDecimal,
    pub avg_entry_price: Option<BigDecimal>,
    pub unrealised_pnl: Option<BigDecimal>,
    pub leverage: Option<BigDecimal>,
    pub liquidation_price: Option<BigDecimal>,
    pub captured_at: DateTime<Utc>,
}

/* -------------------------- BALANCES ----------------------- */

#[derive(Debug, Serialize, Deserialize, FromRow)]
pub struct Balance {
    pub snapshot_id: Uuid,
    pub user_id: i64,
    pub exchange: String,
    pub currency: String,
    pub equity: Option<BigDecimal>,
    pub available: Option<BigDecimal>,
    pub isolated_equity: Option<BigDecimal>,
    pub captured_at: DateTime<Utc>,
}

/* ------------------------- COPY EVENTS --------------------- */

#[derive(Debug, Serialize, Deserialize, FromRow)]
pub struct CopyEvent {
    pub copy_id: Uuid,
    pub leader_order_id: Uuid,
    pub follower_order_id: Uuid,
    pub slippage_bps: Option<BigDecimal>,
    pub copied_at: Option<DateTime<Utc>>,
}

/* -------------------------- AUDIT LOG ---------------------- */

#[derive(Debug, Serialize, Deserialize, FromRow)]
pub struct AuditLog {
    pub event_id: Uuid,
    pub user_id: Option<i64>,
    pub action: String,
    pub details: Option<serde_json::Value>,
    pub ts: Option<DateTime<Utc>>,
}

/* -------------------------- User Strategies ---------------------- */

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct UserStrategy {
    pub strategy_id: Uuid,
    pub user_id: i64,
    pub exchange: String,
    pub symbol: String,
    pub strategy: String,
    pub params: serde_json::Value,
    pub status: String,
    pub created_at: Option<chrono::DateTime<chrono::Utc>>,
}
