// src/routes/strategies.rs
use actix_web::{delete, get, post, web, HttpResponse, Responder, Scope};
use actix_web::dev::HttpServiceFactory;
use serde::Deserialize;
use sqlx::PgPool;
use uuid::Uuid;

use crate::services::scheduler;
use crate::utils::types::ApiResponse;

#[derive(Deserialize)]
pub struct StartReq {
    /// “BTCUSDT”
    symbol: String,
    /// Optional override, e.g. { "period": 30, "sigma": 1.8 }
    params: serde_json::Value,
}

#[post("/mean_reversion")]
async fn start_mean_rev(
    db: web::Data<PgPool>,
    body: web::Json<serde_json::Value>,
) -> impl Responder {
    let row = sqlx::query!(
        r#"INSERT INTO user_strategies (user_id, name, params)
           VALUES (0, 'mean_reversion', $1) RETURNING strategy_id"#,
        body.0
    )
        .fetch_one(db.as_ref())
        .await;

    match row {
        Ok(r) => HttpResponse::Ok().json(ApiResponse::ok(r.strategy_id)),
        Err(e) => {
            log::error!("DB error: {e}");
            HttpResponse::InternalServerError().json(ApiResponse::<()>::err("db error"))
        }
    }
}

#[delete("/{id}")]
async fn stop_strategy(
    db: web::Data<PgPool>,
    path: web::Path<Uuid>,
    auth_user: crate::middleware::auth::DiscordUser,
) -> impl Responder {
    let id = *path;
    let result = sqlx::query!(
        r#"
        UPDATE user_strategies
           SET status = 'disabled'
         WHERE strategy_id = $1
           AND user_id = $2
        "#,
        id,
        auth_user.id
    )
        .execute(db.as_ref())
        .await;

    match result {
        Ok(_) => HttpResponse::Ok().json(ApiResponse::<()>::ok(())),
        Err(e) => {
            log::error!("stop strategy db error: {e}");
            HttpResponse::InternalServerError().json(ApiResponse::<()>::err("db error"))
        }
    }
}

#[get("/active")]
async fn list_active(
    db: web::Data<PgPool>,
    auth_user: crate::middleware::auth::DiscordUser,
) -> impl Responder {
    let rows = sqlx::query!(
        r#"SELECT strategy_id, name, params
             FROM user_strategies
            WHERE user_id = $1 AND status = 'enabled'"#,
        auth_user.id
    )
        .fetch_all(db.as_ref())
        .await;

    match rows {
        Ok(r) => HttpResponse::Ok().json(ApiResponse::ok(r)),
        Err(e) => {
            log::error!("list active db error: {e}");
            HttpResponse::InternalServerError().json(ApiResponse::<()>::err("db error"))
        }
    }
}

pub fn strategy_scope() -> Scope {
    web::scope("/api/strategies")
        .service(start_mean_rev)
        .service(stop_strategy)
        .service(list_active)
}
