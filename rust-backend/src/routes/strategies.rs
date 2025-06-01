// src/routes/strategies.rs
use actix_web::{delete, get, post, web, HttpMessage, HttpRequest, HttpResponse, Responder, Scope};
use serde::Deserialize;
use sqlx::PgPool;
use uuid::Uuid;

use crate::{db::models::UserStrategy, utils::types::ApiResponse};

fn user_id(req: &HttpRequest) -> Result<i64, HttpResponse> {
    req.extensions()
        .get::<String>()
        .and_then(|s| s.parse::<i64>().ok())
        .ok_or_else(|| HttpResponse::Unauthorized().json(ApiResponse::<()>::err("no user id")))
}

#[derive(Deserialize, Debug)]
pub struct StartReq {
    pub exchange: String,
    /// Trading pair, e.g., "BTCUSDT"
    pub symbol: String,
    /// Name of the strategy ("mean_reversion", "trend_follow", etc)
    pub strategy: String,
    /// Params for the strategy (periods, thresholds, etc)
    pub params: serde_json::Value,
}

const ALLOWED_FREE_STRATS: &[&str] = &["mean_reversion", "trend_follow", "vcsr"];

/// Generic “launch strategy” endpoint
#[post("")]
async fn start_strategy(
    req: HttpRequest,
    db: web::Data<PgPool>,
    body: web::Json<StartReq>,
) -> impl Responder {
    let uid = match user_id(&req) {
        Ok(v) => v,
        Err(e) => return e,
    };

    // ─── Tier / plan check ────────────────────────────────────────────────
    // In v1 we assume every user is on the free plan.
    let is_free = true;
    if is_free && !ALLOWED_FREE_STRATS.contains(&body.strategy.as_str()) {
        return HttpResponse::Forbidden().json(ApiResponse::<()>::err(
            "upgrade required for custom strategies",
        ));
    }

    // ─── Insert row ───────────────────────────────────────────────────────
    let row = sqlx::query!(
        r#"
        INSERT INTO user_strategies
              (user_id, exchange, symbol, strategy, params)
        VALUES ($1      , $2      , $3    , $4      , $5)
        RETURNING strategy_id
        "#,
        uid,
        body.exchange,
        body.symbol,
        body.strategy,
        body.params
    )
    .fetch_one(db.as_ref())
    .await;

    match row {
        Ok(r) => HttpResponse::Ok().json(ApiResponse::ok(r.strategy_id)),
        Err(e) => {
            log::error!("start_strategy: DB error: {e}");
            HttpResponse::InternalServerError().json(ApiResponse::<()>::err("db error"))
        }
    }
}

/// DELETE /api/strategies/{id}
#[delete("/{id}")]
async fn stop_strategy(
    req: HttpRequest,
    db: web::Data<PgPool>,
    path: web::Path<Uuid>,
) -> impl Responder {
    let uid = match user_id(&req) {
        Ok(v) => v,
        Err(e) => return e,
    };

    let result = sqlx::query!(
        r#"
        UPDATE user_strategies
           SET status = 'disabled'
         WHERE strategy_id = $1
           AND user_id     = $2
        "#,
        *path,
        uid
    )
    .execute(db.as_ref())
    .await;

    match result {
        Ok(_) => HttpResponse::Ok().json(ApiResponse::<()>::ok(())),
        Err(e) => {
            log::error!("stop_strategy: DB error: {e}");
            HttpResponse::InternalServerError().json(ApiResponse::<()>::err("db error"))
        }
    }
}

/// GET /api/strategies/active
#[get("/active")]
async fn list_active(req: HttpRequest, db: web::Data<PgPool>) -> impl Responder {
    let uid = match user_id(&req) {
        Ok(v) => v,
        Err(e) => return e,
    };

    let rows = sqlx::query_as!(
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
        uid
    )
    .fetch_all(db.as_ref())
    .await;

    match rows {
        Ok(r) => HttpResponse::Ok().json(ApiResponse::ok(r)),
        Err(e) => {
            log::error!("list_active: DB error: {e}");
            HttpResponse::InternalServerError().json(ApiResponse::<()>::err("db error"))
        }
    }
}
pub fn strategy_scope() -> Scope {
    web::scope("/api/strategies")
        .service(start_strategy)
        .service(stop_strategy)
        .service(list_active)
}
