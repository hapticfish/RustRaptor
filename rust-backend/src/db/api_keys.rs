// src/db/api_keys.rs

pub(crate) use crate::db::models::ApiKey;
use sqlx::PgPool;
use uuid::Uuid;
use crate::services::crypto::EnvelopeCrypto;

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
               WHERE user_id = $1 AND exchange = $2"#,
        )
        .bind(user_id)
        .bind(exchange)
        .fetch_optional(db)
        .await
    }

    #[allow(dead_code)]
    pub async fn insert(
        db: &PgPool,
        crypto: &EnvelopeCrypto,
        user_id: i64,
        exchange: &str,
        api_key_plain: &str,
        secret_plain: &str,
        passphrase_plain: Option<&str>,
    ) -> sqlx::Result<Uuid> {
        let (wrapped_key, nonce_k, ct_k) = crypto.seal(api_key_plain.as_bytes());
        let (_, nonce_s, ct_s)          = crypto.seal(secret_plain.as_bytes());
        let (wrapped_pp, nonce_p, ct_p) = if let Some(pp) = passphrase_plain {
            let (wk, n, c) = crypto.seal(pp.as_bytes());
            (Some(wk), Some(n), Some(c))
        } else { (None, None, None) };

        let rec = sqlx::query!(
        r#"INSERT INTO api_keys (
               user_id, exchange,
               encrypted_data_key,
               nonce_key, encrypted_api_key,
               nonce_secret, encrypted_secret,
               nonce_passphrase, encrypted_passphrase
           ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)
           RETURNING key_id"#,
        user_id, exchange,
        wrapped_key,
        nonce_k, ct_k,
        nonce_s, ct_s,
        nonce_p, ct_p
    )
            .fetch_one(db)
            .await?;
        Ok(rec.key_id)
    }
    pub fn decrypt(&self, crypto:&EnvelopeCrypto)
                   -> anyhow::Result<DecryptedApiKey> {
        Ok(DecryptedApiKey {
            api_key: crypto.open(&self.encrypted_data_key,
                                 &self.nonce_key,
                                 &self.encrypted_api_key)?,
            api_secret: crypto.open(&self.encrypted_data_key,
                                    &self.nonce_secret,
                                    &self.encrypted_secret)?,
            api_passphrase: self.encrypted_passphrase.as_ref()
                .zip(self.nonce_passphrase.as_ref())
                .map(|(ct,nonce)| crypto.open(&self.encrypted_data_key, nonce, ct))
                .transpose()?
                .unwrap_or_default(),
        })
    }
}
