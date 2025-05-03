use sqlx::{PgPool, Result};
use crate::db::models::*;
use uuid::Uuid;

// USERS
pub async fn get_user_by_id(pool: &PgPool, user_id: i64) -> Result<Option<User>> {
    sqlx::query_as!(
        User,
        "SELECT * FROM users WHERE user_id = $1",
        user_id
    )
        .fetch_optional(pool)
        .await
}

// API KEYS
pub async fn get_api_keys_for_user(pool: &PgPool, user_id: i64) -> Result<Vec<ApiKey>> {
    sqlx::query_as!(
        ApiKey,
        "SELECT * FROM api_keys WHERE user_id = $1",
        user_id
    )
        .fetch_all(pool)
        .await
}

// STRATEGIES
pub async fn get_active_strategies(pool: &PgPool, user_id: i64) -> Result<Vec<Strategy>> {
    sqlx::query_as!(
        Strategy,
        "SELECT * FROM strategies WHERE user_id = $1 AND active = true",
        user_id
    )
        .fetch_all(pool)
        .await
}

// ORDERS
pub async fn get_orders_by_user(pool: &PgPool, user_id: i64) -> Result<Vec<Order>> {
    sqlx::query_as!(
        Order,
        "SELECT * FROM orders WHERE user_id = $1 ORDER BY opened_at DESC",
        user_id
    )
        .fetch_all(pool)
        .await
}

// FILLS
pub async fn get_fills_for_order(pool: &PgPool, order_id: Uuid) -> Result<Vec<Fill>> {
    sqlx::query_as!(
        Fill,
        "SELECT * FROM fills WHERE order_id = $1 ORDER BY executed_at DESC",
        order_id
    )
        .fetch_all(pool)
        .await
}

// FEES
pub async fn get_fees_for_user(pool: &PgPool, user_id: i64) -> Result<Vec<Fee>> {
    sqlx::query_as!(
        Fee,
        "SELECT * FROM fees WHERE user_id = $1 ORDER BY occurred_at DESC",
        user_id
    )
        .fetch_all(pool)
        .await
}

// POSITIONS
pub async fn get_latest_positions(pool: &PgPool, user_id: i64) -> Result<Vec<Position>> {
    sqlx::query_as!(
        Position,
        "SELECT * FROM positions WHERE user_id = $1 ORDER BY captured_at DESC LIMIT 20",
        user_id
    )
        .fetch_all(pool)
        .await
}

// BALANCES
pub async fn get_latest_balances(pool: &PgPool, user_id: i64) -> Result<Vec<Balance>> {
    sqlx::query_as!(
        Balance,
        "SELECT * FROM balances WHERE user_id = $1 ORDER BY captured_at DESC",
        user_id
    )
        .fetch_all(pool)
        .await
}

// COPY RELATIONS
pub async fn get_copy_followers(pool: &PgPool, leader_id: i64) -> Result<Vec<CopyRelation>> {
    sqlx::query_as!(
        CopyRelation,
        "SELECT * FROM copy_relations WHERE leader_user_id = $1 AND status = 'active'",
        leader_id
    )
        .fetch_all(pool)
        .await
}
