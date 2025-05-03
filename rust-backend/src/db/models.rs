use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use sqlx::FromRow;

#[derive(Debug, Serialize, Deserialize, FromRow)]
pub struct User {
    pub user_id: i64,
    pub rr_username: String,
    pub email: Option<String>,
    pub created_at: Option<DateTime<Utc>>,
}

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

#[derive(Debug, Serialize, Deserialize, FromRow)]
pub struct ExchangeAccount {
    pub acct_id: Uuid,
    pub user_id: i64,
    pub exchange: String,
    pub label: Option<String>,
    pub leverage_default: Option<f64>,
    pub demo: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize, FromRow)]
pub struct Strategy {
    pub strategy_id: Uuid,
    pub user_id: i64,
    pub name: String,
    pub params: serde_json::Value,
    pub active: Option<bool>,
    pub updated_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize, Deserialize, FromRow)]
pub struct CopyRelation {
    pub relation_id: Uuid,
    pub leader_user_id: i64,
    pub follower_user_id: i64,
    pub since: Option<DateTime<Utc>>,
    pub until: Option<DateTime<Utc>>,
    pub status: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, FromRow)]
pub struct Order {
    pub order_id: Uuid,
    pub external_order_id: Option<String>,
    pub user_id: i64,
    pub exchange: String,
    pub market_type: String,
    pub symbol: String,
    pub side: String,
    pub order_type: String,
    pub price: Option<f64>,
    pub size: f64,
    pub reduce_only: Option<bool>,
    pub margin_mode: Option<String>,
    pub position_side: Option<String>,
    pub status: String,
    pub opened_at: Option<DateTime<Utc>>,
    pub closed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize, Deserialize, FromRow)]
pub struct Fill {
    pub fill_id: Uuid,
    pub order_id: Uuid,
    pub maker_taker: String,
    pub fill_price: f64,
    pub fill_size: f64,
    pub trade_fee: Option<f64>,
    pub funding_fee: Option<f64>,
    pub realised_pnl: Option<f64>,
    pub executed_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, FromRow)]
pub struct Fee {
    pub fee_id: Uuid,
    pub user_id: i64,
    pub exchange: String,
    pub symbol: Option<String>,
    pub fee_type: String,
    pub amount: f64,
    pub reference_id: Option<Uuid>,
    pub occurred_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, FromRow)]
pub struct Position {
    pub snapshot_id: Uuid,
    pub user_id: i64,
    pub exchange: String,
    pub symbol: String,
    pub market_type: String,
    pub side: String,
    pub size: f64,
    pub avg_entry_price: Option<f64>,
    pub unrealised_pnl: Option<f64>,
    pub leverage: Option<f64>,
    pub liquidation_price: Option<f64>,
    pub captured_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, FromRow)]
pub struct Balance {
    pub snapshot_id: Uuid,
    pub user_id: i64,
    pub exchange: String,
    pub currency: String,
    pub equity: Option<f64>,
    pub available: Option<f64>,
    pub isolated_equity: Option<f64>,
    pub captured_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, FromRow)]
pub struct CopyEvent {
    pub copy_id: Uuid,
    pub leader_order_id: Uuid,
    pub follower_order_id: Uuid,
    pub slippage_bps: Option<f64>,
    pub copied_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize, Deserialize, FromRow)]
pub struct AuditLog {
    pub event_id: Uuid,
    pub user_id: Option<i64>,
    pub action: String,
    pub details: Option<serde_json::Value>,
    pub ts: Option<DateTime<Utc>>,
}
