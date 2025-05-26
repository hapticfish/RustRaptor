
//  src/services/copy_trading.rs

// use std::{fmt, time::Duration};

use redis::AsyncCommands;
use serde::{Deserialize, Serialize};
use sqlx::{postgres::PgPool};
use uuid::Uuid;
use crate::services::risk;

use crate::{
    db::redis::RedisPool,
    services::trading_engine::{execute_trade, TradeRequest, TradeResponse},
    utils::errors::TradeError,
};

#[derive(thiserror::Error, Debug)]
pub enum CopyError {
    #[error("db: {0}")]
    Db(#[from] sqlx::Error),
    #[error("redis: {0}")]
    Redis(#[from] redis::RedisError),
    #[error("trade: {0}")]
    Trade(#[from] TradeError),
}

/// Persistent model (matches `copy_relations` table)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CopyRelation {
    pub relation_id: Uuid,
    pub leader_user_id: i64,
    pub follower_user_id: i64,
    pub since: chrono::DateTime<chrono::Utc>,
    pub until: Option<chrono::DateTime<chrono::Utc>>,
    pub status: String,
}

/// TTL for Redis follower sets (in seconds)
const FOLLOWER_SET_TTL: usize = 300; // 5 min

//  ================  Public API  ==================================================================

/// Follow a leader.  Persists to Postgres **and** adds follower to Redis set.
///
/// * `leader_id` – Discord snowflake of the leader
/// * `follower_id` – Discord snowflake of the follower
pub async fn add_follower(
    pg: &PgPool,
    redis: &RedisPool,
    leader_id: i64,
    follower_id: i64,
) -> Result<(), CopyError> {

    sqlx::query!(
        r#"
        INSERT INTO copy_relations (leader_user_id, follower_user_id)
        VALUES ($1, $2)
        ON CONFLICT (leader_user_id, follower_user_id, since)
        DO NOTHING
        "#,
        leader_id,
        follower_id
    )
        .execute(pg)
        .await?;


    let key = redis.with_prefix("copy", leader_id);
    let mut conn = redis.connection().await;
    conn.sadd(&key, follower_id).await?;
    conn.expire(&key, FOLLOWER_SET_TTL).await?;
    Ok(())
}

/// Remove follower (soft delete) & update Redis.
pub async fn remove_follower(
    pg: &PgPool,
    redis: &RedisPool,
    leader_id: i64,
    follower_id: i64,
) -> Result<(), CopyError> {
    sqlx::query!(
        r#"
        UPDATE copy_relations
           SET status = 'ended', until = now()
         WHERE leader_user_id = $1
           AND follower_user_id = $2
           AND status = 'active'
        "#,
        leader_id,
        follower_id
    )
        .execute(pg)
        .await?;

    let key = redis.with_prefix("copy", leader_id);
    let mut conn = redis.connection().await;
    conn.srem(&key, follower_id).await?;
    Ok(())
}

/// Returns the current follower list, served from Redis when possible.
pub async fn followers_for_leader(
    pg: &PgPool,
    redis: &RedisPool,
    leader_id: i64,
) -> Result<Vec<i64>, CopyError> {
    let key = redis.with_prefix("copy", leader_id);
    let mut conn = redis.connection().await;

    if let Ok::<Vec<i64>, _>(ids) = conn.smembers(&key).await {
        if !ids.is_empty() {
            return Ok(ids);
        }
    }
    // cache miss → pull from Postgres and repopulate
    let rows: Vec<(i64,)> = sqlx::query_as(
        r#"
        SELECT follower_user_id
          FROM copy_relations
         WHERE leader_user_id = $1
           AND status = 'active'
        "#,
    )
        .bind(leader_id)
        .fetch_all(pg)
        .await?;

    let followers: Vec<i64> = rows.into_iter().map(|r| r.0).collect();
    if !followers.is_empty() {
        conn.sadd(&key, &followers).await?;
        conn.expire(&key, FOLLOWER_SET_TTL).await?;
    }
    Ok(followers)
}

/// Propagate a filled order **from leader** to every follower.
///
///  function is the bridge between the leader’s trading logic and follower replication.
/// In v1 **synchronously** loop – for ≤ ~100 followers.
/// Later: spawn tasks / use a queue.
pub async fn replicate_to_followers(
    pg: &PgPool,
    redis: &RedisPool,
    leader_id: i64,
    leader_fill: &TradeResponse,
    settings: &crate::config::settings::Settings,
) -> Result<(), CopyError> {
    let followers = followers_for_leader(pg, redis, leader_id).await?;

    // -- Grab master key (for decrypting each follower’s API key) --
    let master_key = std::env::var("MASTER_KEY").unwrap_or_default();
    let master_key_bytes = master_key.as_bytes();

    let is_demo = settings.is_demo();

    for fid in followers {

        if let Err(e) = risk::check_drawdown(redis, fid).await {
            log::warn!("follower {fid}: DD limit hit – skipping copy: {e}");
            continue;                                     // just skip this follower
        }

        // naïve 1-for-1 copy; in practice scale, slippage & balance checks apply
        let req = TradeRequest {
            exchange: leader_fill.exchange.clone(),
            symbol: leader_fill.symbol.clone(),
            side: leader_fill.side.clone(),
            order_type: leader_fill.order_type.clone(),
            price: leader_fill.price,
            size: leader_fill.size,
        };

        // Now, execute for the follower!
        if let Err(e) = execute_trade(
            req,
            pg,          // Pass DB connection
            fid,         // Follower's user ID
            is_demo,
            master_key_bytes,
        ).await {
            log::warn!("copy trade for follower {} failed: {}", fid, e);
        }
    }
    Ok(())
}