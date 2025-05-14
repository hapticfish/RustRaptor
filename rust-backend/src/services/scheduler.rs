// src/services/scheduler.rs
use crate::{
    db::redis::RedisPool,
    services::strategies,
    config::settings::Settings,
};
use dashmap::DashMap;
use futures::future::abortable;
use futures::future::AbortHandle;
use sqlx::PgPool;
use uuid::Uuid;

type TaskMap = DashMap<Uuid, AbortHandle>;
static TASKS: once_cell::sync::Lazy<TaskMap> = once_cell::sync::Lazy::new(TaskMap::default);

#[derive(sqlx::FromRow, Clone)]
pub struct StrategyRow {
    pub strategy_id: Uuid,
    pub name: String,
    pub params: serde_json::Value,
}

pub async fn reconcile(pg: &PgPool, redis: &RedisPool, settings: &Settings) -> anyhow::Result<()> {
    let rows: Vec<StrategyRow> = sqlx::query_as::<_, StrategyRow>(
        r#"SELECT strategy_id, name, params
             FROM user_strategies
            WHERE status = 'enabled'"#,
    )
        .fetch_all(pg)
        .await?;

    // --- start missing tasks -------------------------------------------------
    for row in &rows {
        if TASKS.contains_key(&row.strategy_id) {
            continue;
        }
        let redis = redis.clone();
        let settings = settings.clone();
        let row_c = row.clone();

        let (fut, abort) = abortable(tokio::spawn(async move {
            match row_c.name.as_str() {
                "mean_reversion" => {
                    strategies::mean_reversion::loop_forever(row_c, redis, settings).await
                }
                _ => log::warn!("unknown strategy {}", row_c.name),
            }
        }));
        tokio::spawn(fut);               // detach
        TASKS.insert(row.strategy_id, abort);
    }

    // --- stop orphaned tasks -------------------------------------------------
    for id in TASKS.iter().map(|e| *e.key()) {
        if !rows.iter().any(|r| r.strategy_id == id) {
            if let Some((_, abort)) = TASKS.remove(&id) {
                abort.abort();
            }
        }
    }
    Ok(())
}
