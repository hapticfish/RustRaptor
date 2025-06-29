// src/services/blowfin/api.rs
//! BlowFin REST wrapper (trait-friendly for unit tests)
//
//! Production wrappers (`place_order`, `get_balance`) keep their old
//! signatures so nothing upstream breaks; test-harness uses the generic
//! `*_with` versions that accept mock implementations.

use crate::db::api_keys::ApiKey;
use crate::utils::errors::ApiError;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::PgPool;

// ───────────────────────────────────────────────────────────────
// Domain types
// ───────────────────────────────────────────────────────────────
#[derive(Debug, Serialize)]
pub struct OrderRequest {
    #[serde(rename = "instId")]
    pub inst_id: String,
    #[serde(rename = "marginMode")]
    pub margin_mode: String,
    pub side: String,
    #[serde(rename = "orderType")]
    pub order_type: String,
    pub price: Option<String>,
    pub size: String,
}

#[derive(Debug, Deserialize)]
pub struct BlowFinResponse {
    pub code: String,
    #[allow(dead_code)]
    pub msg: String,
    pub data: Value,
}

/// Convenience container returned by the `ApiKeyRepo`
#[derive(Debug, Clone)]
pub struct Credentials {
    pub api_key:        String,
    pub api_secret:     String,
    pub api_passphrase: String,
}

/// ──────────────────────────────────────────────────────────────
///   Seams for mocking
/// ──────────────────────────────────────────────────────────────
#[async_trait::async_trait]
pub trait ApiKeyRepo: Send + Sync {
    async fn fetch_creds(
        &self,
        db: &PgPool,
        user_id: i64,
        master_key: &[u8],
    ) -> Result<Credentials, ApiError>;
}

pub struct ProdApiKeys;
#[async_trait::async_trait]
impl ApiKeyRepo for ProdApiKeys {
    async fn fetch_creds(
        &self,
        db: &PgPool,
        user_id: i64,
        master_key: &[u8],
    ) -> Result<Credentials, ApiError> {
        let row = ApiKey::get_by_user_and_exchange(db, user_id, "blowfin")
            .await?
            .ok_or_else(|| ApiError::Custom("Missing API key for BlowFin".into()))?;
        let c = row
            .decrypt(master_key)
            .map_err(|e| ApiError::Custom(format!("decrypt failed: {e}")))?;
        Ok(Credentials {
            api_key: c.api_key,
            api_secret: c.api_secret,
            api_passphrase: c.api_passphrase,
        })
    }
}

/// Small wrapper around the three “auth” helpers so we can stub them.
pub trait Signer: Send + Sync {
    fn ts(&self) -> String;
    fn nonce(&self) -> String;
    fn sign(
        &self,
        secret: &str,
        method: &str,
        path: &str,
        ts: &str,
        nonce: &str,
        body: &str,
    ) -> String;
}

pub struct ProdSigner;
impl Signer for ProdSigner {
    fn ts(&self) -> String {
        crate::services::blowfin::auth::current_timestamp()
    }
    fn nonce(&self) -> String {
        crate::services::blowfin::auth::generate_nonce()
    }
    fn sign(
        &self,
        secret: &str,
        method: &str,
        path: &str,
        ts: &str,
        nonce: &str,
        body: &str,
    ) -> String {
        crate::services::blowfin::auth::sign_rest(secret, method, path, ts, nonce, body)
    }
}

/// Swappable HTTP client trait
#[async_trait::async_trait]
pub trait Http: Send + Sync {
    async fn post_json<T: serde::de::DeserializeOwned + Send>(
        &self,
        url: &str,
        headers: Vec<(&str, String)>,
        body: &OrderRequest,
    ) -> Result<T, ApiError>;

    async fn get_json<T: serde::de::DeserializeOwned + Send>(
        &self,
        url: &str,
        headers: Vec<(&str, String)>,
    ) -> Result<T, ApiError>;
}

pub struct ReqwestClient;
#[async_trait::async_trait]
impl Http for ReqwestClient {
    async fn post_json<T: serde::de::DeserializeOwned + Send>(
        &self,
        url: &str,
        headers: Vec<(&str, String)>,
        body: &OrderRequest,
    ) -> Result<T, ApiError> {
        let client = Client::new();
        let mut req = client.post(url);
        for (k, v) in headers {
            req = req.header(k, v);
        }
        Ok(req.json(body).send().await?.json::<T>().await?)
    }

    async fn get_json<T: serde::de::DeserializeOwned + Send>(
        &self,
        url: &str,
        headers: Vec<(&str, String)>,
    ) -> Result<T, ApiError> {
        let client = Client::new();
        let mut req = client.get(url);
        for (k, v) in headers {
            req = req.header(k, v);
        }
        Ok(req.send().await?.json::<T>().await?)
    }
}

// ──────────────────────────────────────────────────────────────
//  Generic helpers (unit-testable)
// ──────────────────────────────────────────────────────────────
#[allow(clippy::too_many_arguments)]
pub async fn place_order_with<
    K: ApiKeyRepo,
    S: Signer,
    H: Http,
>(
    db: &PgPool,
    user_id: i64,
    order: &OrderRequest,
    is_demo: bool,
    master_key: &[u8],
    keys: &K,
    signer: &S,
    http: &H,
) -> Result<BlowFinResponse, ApiError> {
    // ------------------------------------------------------------------
    // 1. Resolve URL
    let path = "/api/v1/trade/order";
    let base = if is_demo {
        "https://demo-trading-openapi.blofin.com"
    } else {
        "https://openapi.blofin.com"
    };
    let url = format!("{base}{path}");

    // ------------------------------------------------------------------
    // 2. Credentials
    let cred = keys.fetch_creds(db, user_id, master_key).await?;

    // ------------------------------------------------------------------
    // 3. Sign & headers
    let ts = signer.ts();
    let nonce = signer.nonce();
    let body = serde_json::to_string(order)?;
    let sig = signer.sign(&cred.api_secret, "POST", path, &ts, &nonce, &body);

    let headers = vec![
        ("ACCESS-KEY", cred.api_key),
        ("ACCESS-SIGN", sig),
        ("ACCESS-TIMESTAMP", ts),
        ("ACCESS-NONCE", nonce),
        ("ACCESS-PASSPHRASE", cred.api_passphrase),
    ];

    // ------------------------------------------------------------------
    // 4. HTTP POST
    http.post_json::<BlowFinResponse>(&url, headers, order).await
}

#[allow(clippy::too_many_arguments)]
pub async fn get_balance_with<
    K: ApiKeyRepo,
    S: Signer,
    H: Http,
>(
    db: &PgPool,
    user_id: i64,
    is_demo: bool,
    master_key: &[u8],
    keys: &K,
    signer: &S,
    http: &H,
) -> Result<BlowFinResponse, ApiError> {
    let path = "/api/v1/asset/balances?accountType=futures";
    let base = if is_demo {
        "https://demo-trading-openapi.blofin.com"
    } else {
        "https://openapi.blofin.com"
    };
    let url = format!("{base}{path}");

    let cred = keys.fetch_creds(db, user_id, master_key).await?;

    let ts = signer.ts();
    let nonce = signer.nonce();
    let sig = signer.sign(&cred.api_secret, "GET", path, &ts, &nonce, "");

    let headers = vec![
        ("ACCESS-KEY", cred.api_key),
        ("ACCESS-SIGN", sig),
        ("ACCESS-TIMESTAMP", ts),
        ("ACCESS-NONCE", nonce),
        ("ACCESS-PASSPHRASE", cred.api_passphrase),
    ];

    http.get_json::<BlowFinResponse>(&url, headers).await
}

// ──────────────────────────────────────────────────────────────
//  Production wrappers (unchanged signatures)
// ──────────────────────────────────────────────────────────────
pub async fn place_order(
    db: &PgPool,
    user_id: i64,
    order: &OrderRequest,
    is_demo: bool,
    master_key: &[u8],
) -> Result<BlowFinResponse, ApiError> {
    place_order_with(
        db,
        user_id,
        order,
        is_demo,
        master_key,
        &ProdApiKeys,
        &ProdSigner,
        &ReqwestClient,
    )
        .await
}

pub async fn get_balance(
    db: &PgPool,
    user_id: i64,
    is_demo: bool,
    master_key: &[u8],
) -> Result<BlowFinResponse, ApiError> {
    get_balance_with(
        db,
        user_id,
        is_demo,
        master_key,
        &ProdApiKeys,
        &ProdSigner,
        &ReqwestClient,
    )
        .await
}

// ======================================================================
// UNIT TESTS
// ======================================================================
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use sqlx::{postgres::PgPoolOptions, PgPool};

    /// ——— In-memory PgPool stub (never contacted) ———
    fn lazy_pg() -> PgPool {
        PgPoolOptions::new()
            .max_connections(1)
            .connect_lazy("postgres://unused:unused@localhost/unused")
            .expect("lazy PgPool")
    }

    /// ——— Mock ApiKeyRepo ———
    struct MockKeys {
        bad_decrypt: bool,
    }
    #[async_trait::async_trait]
    impl ApiKeyRepo for MockKeys {
        async fn fetch_creds(
            &self,
            _db: &PgPool,
            _uid: i64,
            _k: &[u8],
        ) -> Result<Credentials, ApiError> {
            if self.bad_decrypt {
                Err(ApiError::Custom("decrypt failed".into()))
            } else {
                Ok(Credentials {
                    api_key: "AK".into(),
                    api_secret: "SK".into(),
                    api_passphrase: "PW".into(),
                })
            }
        }
    }

    /// ——— Deterministic Signer ———
    struct MockSigner;
    impl Signer for MockSigner {
        fn ts(&self) -> String       { "TS".into() }
        fn nonce(&self) -> String    { "NN".into() }
        fn sign(
            &self,
            _s:&str,_m:&str,_p:&str,_t:&str,_n:&str,_b:&str
        )->String { "SIGN".into() }
    }

    /// ——— Capturing HTTP stub ———
    struct StubHttp {
        last_url:   std::sync::Mutex<String>,
        last_hdrs:  std::sync::Mutex<Vec<(String,String)>>,
        hit_post:   std::sync::Mutex<u32>,
        hit_get:    std::sync::Mutex<u32>,
        code:       &'static str,
    }
    impl StubHttp {
        fn new(code:&'static str)->Self{
            Self{ last_url:Default::default(), last_hdrs:Default::default(),
                hit_post:Default::default(), hit_get:Default::default(), code }
        }
    }
    #[async_trait::async_trait]
    impl Http for StubHttp {
        async fn post_json<T: serde::de::DeserializeOwned + Send>(
            &self,u:&str,h:Vec<(&str,String)>,_b:&OrderRequest
        )->Result<T,ApiError>{
            *self.hit_post.lock().unwrap()+=1;
            *self.last_url.lock().unwrap()=u.into();
            *self.last_hdrs.lock().unwrap()=h.iter().map(|(k,v)|(k.to_string(),v.clone())).collect();
            let resp=json!({"code":self.code,"msg":"","data":{"order_id":"X"}}).to_string();
            Ok(serde_json::from_str(&resp)?)
        }
        async fn get_json<T: serde::de::DeserializeOwned + Send>(
            &self,u:&str,h:Vec<(&str,String)>,
        )->Result<T,ApiError>{
            *self.hit_get.lock().unwrap()+=1;
            *self.last_url.lock().unwrap()=u.into();
            *self.last_hdrs.lock().unwrap()=h.iter().map(|(k,v)|(k.to_string(),v.clone())).collect();
            let resp=json!({"code":self.code,"msg":"","data":{"bal":123}}).to_string();
            Ok(serde_json::from_str(&resp)?)
        }
    }

    /// Common sample order
    fn order() -> OrderRequest {
        OrderRequest {
            inst_id: "BTCUSDT".into(),
            margin_mode: "isolated".into(),
            side: "buy".into(),
            order_type: "market".into(),
            price: None,
            size: "1".into(),
        }
    }

    // ——————————————————————————————————————————
    // Happy path POST
    // ——————————————————————————————————————————
    #[tokio::test]
    async fn post_place_order_ok() {
        let db = lazy_pg();
        let http = StubHttp::new("0");
        let resp = place_order_with(
            &db, 42, &order(), false, b"K", &MockKeys{bad_decrypt:false},
            &MockSigner, &http
        ).await.expect("ok");

        assert_eq!(resp.code, "0");
        assert_eq!(*http.hit_post.lock().unwrap(), 1);
        assert!(http.last_url.lock().unwrap().contains("/trade/order"));
        assert!(http.last_hdrs.lock().unwrap()
            .iter().any(|(k,v)| k=="ACCESS-SIGN" && v=="SIGN"));
    }

    // ——————————————————————————————————————————
    // Credential failure bubbles up
    // ——————————————————————————————————————————
    #[tokio::test]
    async fn decrypt_error_propagates() {
        let db = lazy_pg();
        let http = StubHttp::new("0");
        let err = place_order_with(
            &db, 1, &order(), true, b"K",
            &MockKeys{bad_decrypt:true}, &MockSigner, &http
        ).await.unwrap_err();

        match err {
            ApiError::Custom(m) => assert!(m.contains("decrypt")),
            _ => panic!("wrong error: {err:?}"),
        }
        assert_eq!(*http.hit_post.lock().unwrap(), 0);     // short-circuited
    }

    // ——————————————————————————————————————————
    // GET balance path
    // ——————————————————————————————————————————
    #[tokio::test]
    async fn get_balance_ok() {
        let db = lazy_pg();
        let http = StubHttp::new("0");
        let resp = get_balance_with(
            &db, 7, true, b"K",
            &MockKeys{bad_decrypt:false}, &MockSigner, &http
        ).await.unwrap();

        assert_eq!(resp.code, "0");
        assert_eq!(*http.hit_get.lock().unwrap(), 1);
        assert_eq!(resp.data["bal"], json!(123));
    }
}
