// src/db/api_keys.rs

use sqlx::{PgPool};
use uuid::Uuid;
pub(crate) use crate::db::models::ApiKey;

/// **Optional**: Public struct to use when returning decrypted data
#[derive(Debug, Clone)]
pub struct DecryptedApiKey {
    pub api_key: String,
    pub api_secret: String,
    pub api_passphrase: String,
}

impl ApiKey {
    /// Fetch a user’s API key record for a specific exchange.
    pub async fn get_by_user_and_exchange(
        db: &PgPool,
        user_id: i64,
        exchange: &str,
    ) -> sqlx::Result<Option<ApiKey>> {
        sqlx::query_as::<_, ApiKey>(
            r#"SELECT * FROM api_keys
               WHERE user_id = $1 AND exchange = $2"#
        )
            .bind(user_id)
            .bind(exchange)
            .fetch_optional(db)
            .await
    }



    /// Insert new API key record (returns generated UUID).
    pub async fn insert(
        db: &PgPool,
        user_id: i64,
        exchange: &str,
        api_key: Vec<u8>,
        secret: Vec<u8>,
        passphrase: Option<Vec<u8>>,
    ) -> sqlx::Result<Uuid> {
        let rec = sqlx::query!(
            r#"INSERT INTO api_keys (
                   user_id, exchange,
                   encrypted_api_key,
                   encrypted_secret,
                   encrypted_passphrase
               )
               VALUES ($1, $2, $3, $4, $5)
               RETURNING key_id"#,
            user_id,
            exchange,
            api_key,
            secret,
            passphrase
        )
            .fetch_one(db)
            .await?;

        Ok(rec.key_id)
    }pub fn decrypt(&self, _master_key: &[u8]) -> Result<DecryptedApiKey, Box<dyn std::error::Error>> {
        // Stub: replace with real crypto logic (AES256, etc.)
        Ok(DecryptedApiKey {
            api_key: String::from_utf8(self.encrypted_api_key.clone())?,      // Real code: decrypt bytes
            api_secret: String::from_utf8(self.encrypted_secret.clone())?,
            api_passphrase: self.encrypted_passphrase.as_ref()
                .map(|b| String::from_utf8(b.clone()))
                .transpose()?
                .unwrap_or_default(),
        })
    }
}