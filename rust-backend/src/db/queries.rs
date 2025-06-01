use crate::{
    db::models::*,
    utils::types::{FeeType, MakerTaker, MarketType, OrderStatus, OrderType},
};
use sqlx::{PgPool, Result};
use uuid::Uuid;

/* -----------------------  USERS  ----------------------- */
#[allow(dead_code)]
pub async fn get_user_by_id(pool: &PgPool, user_id: i64) -> Result<Option<User>> {
    sqlx::query_as!(
        User,
        r#"
        SELECT user_id, rr_username, email, created_at
        FROM   users
        WHERE  user_id = $1
        "#,
        user_id
    )
    .fetch_optional(pool)
    .await
}

/* ---------------------- API KEYS ----------------------- */
#[allow(dead_code)]
pub async fn get_api_keys_for_user(pool: &PgPool, user_id: i64) -> Result<Vec<ApiKey>> {
    sqlx::query_as!(
        ApiKey,
        r#"
        SELECT key_id, user_id, exchange,
               encrypted_api_key, encrypted_secret, encrypted_passphrase,
               created_at
        FROM   api_keys
        WHERE  user_id = $1
        "#,
        user_id
    )
    .fetch_all(pool)
    .await
}

/* --------------------- STRATEGIES ---------------------- */
#[allow(dead_code)]
pub async fn get_active_strategies(pool: &PgPool, user_id: i64) -> Result<Vec<UserStrategy>> {
    sqlx::query_as!(
        UserStrategy,
        r#"
        SELECT strategy_id,
               user_id,
               exchange,
               symbol,
               strategy,
               params,
               status,
               created_at
        FROM   user_strategies
        WHERE  user_id = $1
          AND  status  = 'enabled'
        "#,
        user_id
    )
    .fetch_all(pool)
    .await
}

/* ───────── ORDERS ──────── */
#[allow(dead_code)]
pub async fn get_orders_by_user(pool: &PgPool, user_id: i64) -> Result<Vec<Order>> {
    sqlx::query_as!(
        Order,
        r#"
        SELECT order_id,
               external_order_id,
               user_id,
               exchange,
               market_type  AS "market_type!: MarketType",
               symbol,
               side,
               order_type   AS "order_type!: OrderType",
               price        AS "price:      sqlx::types::BigDecimal",
               size         AS "size:       sqlx::types::BigDecimal",
               reduce_only,
               margin_mode,
               position_side,
               status       AS "status!:    OrderStatus",
               opened_at,
               closed_at
        FROM   orders
        WHERE  user_id = $1
        ORDER  BY opened_at DESC
        "#,
        user_id
    )
    .fetch_all(pool)
    .await
}

/* ───────── FILLS ───────── */
#[allow(dead_code)]
pub async fn get_fills_for_order(pool: &PgPool, order_id: Uuid) -> Result<Vec<Fill>> {
    sqlx::query_as!(
        Fill,
        r#"
        SELECT fill_id,
               order_id,
               maker_taker   AS "maker_taker!: MakerTaker",
               fill_price    AS "fill_price:    sqlx::types::BigDecimal",
               fill_size     AS "fill_size:     sqlx::types::BigDecimal",
               trade_fee     AS "trade_fee:     sqlx::types::BigDecimal",
               funding_fee   AS "funding_fee:   sqlx::types::BigDecimal",
               realised_pnl  AS "realised_pnl:  sqlx::types::BigDecimal",
               executed_at
        FROM   fills
        WHERE  order_id = $1
        ORDER  BY executed_at DESC
        "#,
        order_id
    )
    .fetch_all(pool)
    .await
}

/* ───────── FEES ────────── */
#[allow(dead_code)]
pub async fn get_fees_for_user(pool: &PgPool, user_id: i64) -> Result<Vec<Fee>> {
    sqlx::query_as!(
        Fee,
        r#"
        SELECT fee_id,
               user_id,
               exchange,
               symbol,
               fee_type   AS "fee_type!:  FeeType",
               amount     AS "amount:    sqlx::types::BigDecimal",
               reference_id,
               occurred_at
        FROM   fees
        WHERE  user_id = $1
        ORDER  BY occurred_at DESC
        "#,
        user_id
    )
    .fetch_all(pool)
    .await
}

/* ─────── POSITIONS ─────── */
#[allow(dead_code)]
pub async fn get_latest_positions(pool: &PgPool, user_id: i64) -> Result<Vec<Position>> {
    sqlx::query_as!(
        Position,
        r#"
        SELECT snapshot_id,
               user_id,
               exchange,
               symbol,
               market_type       AS "market_type!: MarketType",
               side,
               size              AS "size:              sqlx::types::BigDecimal",
               avg_entry_price   AS "avg_entry_price:   sqlx::types::BigDecimal",
               unrealised_pnl    AS "unrealised_pnl:    sqlx::types::BigDecimal",
               leverage          AS "leverage:          sqlx::types::BigDecimal",
               liquidation_price AS "liquidation_price: sqlx::types::BigDecimal",
               captured_at
        FROM   positions
        WHERE  user_id = $1
        ORDER  BY captured_at DESC
        LIMIT  20
        "#,
        user_id
    )
    .fetch_all(pool)
    .await
}

/* ─────── BALANCES ──────── */
#[allow(dead_code)]
pub async fn get_latest_balances(pool: &PgPool, user_id: i64) -> Result<Vec<Balance>> {
    sqlx::query_as!(
        Balance,
        r#"
        SELECT snapshot_id,
               user_id,
               exchange,
               currency,
               equity           AS "equity:           sqlx::types::BigDecimal",
               available        AS "available:        sqlx::types::BigDecimal",
               isolated_equity  AS "isolated_equity:  sqlx::types::BigDecimal",
               captured_at
        FROM   balances
        WHERE  user_id = $1
        ORDER  BY captured_at DESC
        "#,
        user_id
    )
    .fetch_all(pool)
    .await
}

/* -------------------- COPY RELATIONS ------------------- */
#[allow(dead_code)]
pub async fn get_copy_followers(pool: &PgPool, leader_id: i64) -> Result<Vec<CopyRelation>> {
    sqlx::query_as!(
        CopyRelation,
        r#"
        SELECT relation_id,
               leader_user_id,
               follower_user_id,
               since,
               until,
               status
        FROM   copy_relations
        WHERE  leader_user_id = $1
        AND    status = 'active'
        "#,
        leader_id
    )
    .fetch_all(pool)
    .await
}
