//  src/db/redis.rs

use std::{sync::Arc, time::Instant};
use redis::{
    aio::{ConnectionManager},
    AsyncCommands, Client, RedisError, ToRedisArgs,
};
use serde::{de::DeserializeOwned, Serialize};

/// Thin, cheap-to-clone handle.
#[derive(Clone)]
pub struct RedisPool {
    manager: Arc<ConnectionManager>,
}

impl RedisPool {
    /// Build once at start-up and share via `.data()` in Actix.
    pub async fn new(url: &str) -> Result<Self, RedisError> {
        let client = Client::open(url)?;
        let manager    = client.get_connection_manager().await?;
        Ok(Self {
            manager: Arc::new(manager),
        })
    }

    pub(crate) fn manager(&self) -> Arc<ConnectionManager> {
        self.manager.clone()
    }


    // ─── Helpers ──────────────────────────────────────────────────────────────
    pub async fn set_json<K, T>(
        &self,
        key: K,
        value: &T,
        ttl_secs: usize,
    ) -> Result<(), RedisError>
    where
        K: ToRedisArgs + Send + Sync,
        T: Serialize,
    {
        let mut con = self.manager().as_ref().clone();
        let payload = serde_json::to_string(value)
            .map_err(|e| RedisError::from((redis::ErrorKind::TypeError, "serde", e.to_string())))?;

        let started = Instant::now();
        if ttl_secs == 0 {
            redis::cmd("SET").arg(key).arg(payload).query_async::<_, ()>(&mut con).await?;
        } else {
            redis::cmd("SET")
                .arg(key)
                .arg(payload)
                .arg("EX")
                .arg(ttl_secs)
                .query_async::<_, ()>(&mut con)
                .await?;
        }
        log::debug!("redis SET took {:?}", started.elapsed());
        Ok(())
    }

    pub async fn get_json<K, T>(&self, key: K) -> Result<Option<T>, RedisError>
    where
        K: ToRedisArgs + Send + Sync,
        T: DeserializeOwned,
    {
        let mut con = self.manager().as_ref().clone();
        let started = Instant::now();
        let raw: Option<String> = con.get(key).await?;
        log::debug!("redis GET took {:?}", started.elapsed());

        match raw {
            Some(s) => Ok(Some(serde_json::from_str(&s).map_err(|e| {
                RedisError::from((redis::ErrorKind::TypeError, "serde", e.to_string()))
            })?)),
            None => Ok(None),
        }
    }

    /// Uniformly names-space keys:  `"copy:12345"`
    pub fn with_prefix(&self, prefix: &str, key: impl AsRef<str>) -> String {
        format!("{prefix}:{}", key.as_ref())
    }
}
