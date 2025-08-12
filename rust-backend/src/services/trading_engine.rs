// src/services/trading_engine.rs
//! Thin execution layer that routes *validated* trade requests to the
//! exchange client, handles risk checks, and post-processes the response.
//!
//! The production path still hard-wires Blowfin + risk, but all external
//! calls are now routed through *traits* so the unit-tests can inject mocks
//! without `unsafe` or global state hacks.

use redis::Client;
use serde_json::Value;
use sqlx::PgPool;
use crate::{
    db::api_keys::ApiKey,
    services::{
        blowfin::{
            api::OrderRequest,
            client::BlowfinClient,
        },
        crypto::GLOBAL_CRYPTO,
        risk,
    },
    utils::errors::TradeError,
};

// ──────────────────────────────────────────────────────────────
// Public types
// ──────────────────────────────────────────────────────────────
#[derive(Debug, Clone, serde::Serialize)]
pub enum Exchange {
    Blowfin,
    // placeholder for future variants
}

#[derive(Debug)]
pub struct TradeRequest {
    pub exchange: Exchange,
    pub symbol: String,
    pub side: String,
    pub order_type: String,
    pub price: Option<f64>,
    pub size: f64,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct TradeResponse {
    pub success: bool,
    pub exchange: Exchange,
    pub symbol: String,
    pub side: String,
    pub order_type: String,
    pub price: Option<f64>,
    pub size: f64,
    pub data: Value,
}

// ──────────────────────────────────────────────────────────────
//  Dependency-injection seams  (traits + prod impls)
// ──────────────────────────────────────────────────────────────
#[async_trait::async_trait]
pub trait RiskGuard: Send + Sync {
    fn check_slippage(&self, slip: f64) -> Result<(), TradeError>;
}

pub struct ProdRisk;
#[async_trait::async_trait]
impl RiskGuard for ProdRisk {
    fn check_slippage(&self, slip: f64) -> Result<(), TradeError> {
        risk::check_slippage(slip)
    }
}

#[derive(Debug)]
pub struct ApiResponse {
    pub code: String,
    pub data: Value,
}

#[async_trait::async_trait]
pub trait ApiClient: Send + Sync {
    async fn place_order(
        &self,
        db: &PgPool,
        user_id: i64,
        order: &OrderRequest,
        is_demo: bool,
        master_key: &[u8],
    ) -> Result<ApiResponse, TradeError>;
}

// ──────────────────────────────────────────────────────────────
//  Generic core  (unit-testable)
// ──────────────────────────────────────────────────────────────
#[allow(clippy::too_many_arguments)]
pub async fn execute_trade_with<R: RiskGuard, A: ApiClient>(
    req: TradeRequest,
    db: &PgPool,
    user_id: i64,
    is_demo: bool,
    master_key: &[u8],
    risk: &R,
    api: &A,
) -> Result<TradeResponse, TradeError> {
    // 1. Pre-trade slippage/risk check
    risk.check_slippage(0.0)?;

    // 2. Build outbound order & call the API
    let order_req = OrderRequest {
        inst_id: req.symbol.clone(),
        margin_mode: "isolated".into(),
        side: req.side.clone(),
        order_type: req.order_type.clone(),
        price: req.price.map(|p| p.to_string()),
        size: req.size.to_string(),
    };

    let api_resp = api
        .place_order(db, user_id, &order_req, is_demo, master_key)
        .await?;

    // 3. Shape into canonical response
    Ok(TradeResponse {
        success: api_resp.code == "0",
        exchange: req.exchange.clone(),
        symbol: req.symbol,
        side: req.side,
        order_type: req.order_type,
        price: req.price,
        size: req.size,
        data: api_resp.data,
    })
}

// ──────────────────────────────────────────────────────────────
//  Production wrapper (keeps current call-sites unchanged)
// ──────────────────────────────────────────────────────────────
pub async fn execute_trade(
    req: TradeRequest,
    db: &PgPool,
    user_id: i64,
    is_demo: bool,
    master_key: &[u8],
) -> Result<TradeResponse, TradeError> {
    // 1) fetch & decrypt creds
    let row = ApiKey::get_by_user_and_exchange(db, user_id, "blowfin")
        .await
        .map_err(|e| TradeError::Db(e.into()))?        // ← NEW: convert sqlx::Error ➜ TradeError
        .ok_or(TradeError::MissingKey)?;
    let creds = row.decrypt(&GLOBAL_CRYPTO)
        .map_err(|e| TradeError::Api(e.into()))?;                         // map into TradeError

    let adapter = BlowfinClient::new(creds);

    execute_trade_with(
        req, db, user_id, is_demo, master_key, &ProdRisk, &adapter,
    ).await
}

// ======================================================================
// UNIT TESTS
// ======================================================================
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use sqlx::postgres::PgPoolOptions;
    use sqlx::PgPool;
    use std::sync::atomic::{AtomicUsize, Ordering};

    // Helper: in-memory Sqlite pool (sqlx works w/o DB schema for our tests)
    fn lazy_pg_pool() -> PgPool {
        // `connect_lazy` does **not** open a socket until the pool is first used,
        // and the mock API never touches the DB, so this is zero-cost.
        PgPoolOptions::new()
            .max_connections(1)
            .connect_lazy("postgres://unused:unused@localhost/unused")
            .expect("lazy PgPool")
    }

    // ────────────── Mock RiskGuard ──────────────
    struct MockRisk {
        fail: bool,
        calls: AtomicUsize,
    }
    impl MockRisk {
        fn ok() -> Self {
            Self {
                fail: false,
                calls: AtomicUsize::new(0),
            }
        }
        fn err() -> Self {
            Self {
                fail: true,
                calls: AtomicUsize::new(0),
            }
        }
    }
    #[async_trait::async_trait]
    impl RiskGuard for MockRisk {
        fn check_slippage(&self, _s: f64) -> Result<(), TradeError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            if self.fail {
                Err(TradeError::RiskViolation("slippage too high".into()))
            } else {
                Ok(())
            }
        }
    }

    // ────────────── Mock ApiClient ──────────────
    struct MockApi {
        code: &'static str,
        order_seen: AtomicUsize,
    }
    #[async_trait::async_trait]
    impl ApiClient for MockApi {
        async fn place_order(
            &self,
            _db: &PgPool,
            _uid: i64,
            _o: &OrderRequest,
            _demo: bool,
            _k: &[u8],
        ) -> Result<ApiResponse, TradeError> {
            self.order_seen.fetch_add(1, Ordering::SeqCst);
            Ok(ApiResponse {
                code: self.code.into(),
                data: json!({"order_id":"MOCK123"}),
            })
        }
    }

    // ────────────── Common builder ──────────────
    fn sample_req() -> TradeRequest {
        TradeRequest {
            exchange: Exchange::Blowfin,
            symbol: "BTCUSDT".into(),
            side: "buy".into(),
            order_type: "market".into(),
            price: Some(25_000.0),
            size: 0.3,
        }
    }

    // ────────────────────────────────────────────
    // Happy path: success == true, all fields OK
    // ────────────────────────────────────────────
    #[tokio::test]
    async fn happy_path_executes_trade() {
        let db = lazy_pg_pool();
        let api = MockApi {
            code: "0",
            order_seen: AtomicUsize::new(0),
        };
        let risk = MockRisk::ok();

        let resp = execute_trade_with(sample_req(), &db, 99, false, b"key", &risk, &api)
            .await
            .expect("trade failed");

        assert!(resp.success);
        assert_eq!(resp.symbol, "BTCUSDT");
        assert_eq!(risk.calls.load(Ordering::SeqCst), 1);
        assert_eq!(api.order_seen.load(Ordering::SeqCst), 1);
        assert_eq!(resp.data["order_id"], "MOCK123");
    }

    // ────────────────────────────────────────────
    // API returns code != "0"  → success == false
    // ────────────────────────────────────────────
    #[tokio::test]
    async fn api_error_sets_success_false() {
        let db = lazy_pg_pool();
        let api = MockApi {
            code: "1001",
            order_seen: AtomicUsize::new(0),
        };
        let risk = MockRisk::ok();

        let resp = execute_trade_with(sample_req(), &db, 1, true, b"k", &risk, &api)
            .await
            .unwrap();

        assert!(!resp.success);
        assert_eq!(api.order_seen.load(Ordering::SeqCst), 1);
    }

    // ────────────────────────────────────────────
    // Risk layer blocks before API is called
    // ────────────────────────────────────────────
    #[tokio::test]
    async fn risk_failure_short_circuits() {
        let db = lazy_pg_pool();
        let api = MockApi {
            code: "0",
            order_seen: AtomicUsize::new(0),
        };
        let risk = MockRisk::err();

        let err = execute_trade_with(sample_req(), &db, 1, false, b"k", &risk, &api)
            .await
            .unwrap_err();

        assert!(
            matches!(err, TradeError::RiskViolation(_)),
            "expected risk violation, got {err:?}"
        );
        assert_eq!(risk.calls.load(Ordering::SeqCst), 1);
        assert_eq!(api.order_seen.load(Ordering::SeqCst), 0);
    }

    // ────────────────────────────────────────────
    // Future-proofing: new enum variant placeholder
    // ────────────────────────────────────────────
    #[test]
    fn exchange_enum_is_exhaustive() {
        // Compile-time only – will fail to compile if a new variant
        // is added without updating this match.
        fn _cover(e: Exchange) {
            match e {
                Exchange::Blowfin => {}
            }
        }
    }
}
