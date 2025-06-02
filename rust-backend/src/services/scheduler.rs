use crate::{
    config::settings::Settings,
    db::redis::RedisPool,
    services::{market_data::MarketBus, strategies},
};
use dashmap::DashMap;
use futures::future::{abortable, AbortHandle};
use sqlx::PgPool;
use std::sync::Arc;
use uuid::Uuid;

type TaskMap = DashMap<Uuid, AbortHandle>;
static TASKS: once_cell::sync::Lazy<TaskMap> = once_cell::sync::Lazy::new(TaskMap::default);

#[derive(sqlx::FromRow, Clone, Default)]
pub struct StrategyRow {
    pub strategy_id: Uuid,
    pub user_id: i64,
    pub exchange: String,
    pub symbol: String,
    pub strategy: String,
    pub params: serde_json::Value,
}

pub async fn reconcile(
    pg: &PgPool,
    redis: &RedisPool,
    settings: &Settings,
    bus: &MarketBus,
) -> anyhow::Result<()> {
    // ---------------------------------------------------------
    // 1. Fetch enabled rows
    // ---------------------------------------------------------
    let rows: Vec<StrategyRow> = sqlx::query_as!(
        StrategyRow,
        r#"
        SELECT strategy_id,
               user_id,
               exchange,
               symbol,
               strategy,
               params
        FROM   user_strategies
        WHERE  status = 'enabled'
        "#
    )
    .fetch_all(pg)
    .await?;

    let master_key = std::env::var("MASTER_KEY").unwrap_or_default().into_bytes();
    let is_demo = settings.is_demo();

    // ---------------------------------------------------------
    // 2. Spawn missing tasks
    // ---------------------------------------------------------
    for row in &rows {
        if TASKS.contains_key(&row.strategy_id) {
            continue;
        }

        let r = row.clone();
        let rd = redis.clone();
        let bus_clone = bus.clone();
        let db = pg.clone();
        let master_key = master_key.clone();

        let (task, abort) = abortable(tokio::spawn(async move {
            match r.strategy.as_str() {
                "mean_reversion" => {
                    strategies::mean_reversion::loop_forever(
                        r,
                        rd,
                        Arc::new(db),
                        bus_clone,
                        master_key,
                        is_demo,
                    )
                    .await
                }
                "trend_follow" => {
                    strategies::trend_follow::loop_forever(
                        r,
                        rd,
                        Arc::new(db),
                        bus_clone,
                        master_key,
                        is_demo,
                    )
                    .await
                }
                "vcsr" => {
                    strategies::vcsr::loop_forever(
                        r,
                        rd,
                        Arc::new(db),
                        bus_clone,
                        master_key,
                        is_demo,
                    )
                    .await
                }
                other => log::warn!("scheduler: unknown strategy '{other}'"),
            }
        }));

        tokio::spawn(task);
        TASKS.insert(row.strategy_id, abort);
    }

    // ---------------------------------------------------------
    // 3. Reap tasks whose DB row disappeared / disabled
    // ---------------------------------------------------------
    for id in TASKS.iter().map(|e| *e.key()) {
        if !rows.iter().any(|r| r.strategy_id == id) {
            if let Some((_, abort)) = TASKS.remove(&id) {
                abort.abort();
            }
        }
    }

    Ok(())
}
