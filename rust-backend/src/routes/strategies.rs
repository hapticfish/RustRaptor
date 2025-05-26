// src/routes/strategies.rs
use actix_web::{delete, get, post, web, HttpResponse, Responder, Scope, HttpRequest, HttpMessage};
// use actix_web::dev::HttpServiceFactory;
use crate::db::models::UserStrategy;
use serde::Deserialize;
use sqlx::PgPool;
use uuid::Uuid;
use crate::utils::types::ApiResponse;


fn user_id(req: &HttpRequest) -> Result<i64, HttpResponse> {
    req.extensions()
        .get::<String>()
        .and_then(|s| s.parse::<i64>().ok())
        .ok_or_else(|| HttpResponse::Unauthorized().json(ApiResponse::<()>::err("no user id")))
}

#[derive(Deserialize)]
pub struct StartReq {
    /// “BTCUSDT”
    symbol: String,
    /// Optional override, e.g. { "period": 30, "sigma": 1.8 }
    params: serde_json::Value,
}

#[post("/mean_reversion")]
async fn start_mean_rev(
    req: HttpRequest,
    db: web::Data<PgPool>,
    body: web::Json<serde_json::Value>,
) -> impl Responder {
    let uid = match user_id(&req) {
        Ok(v) => v,
        Err(e) => return e,
    };

    let row = sqlx::query!(
        r#"INSERT INTO user_strategies (user_id, name, params)
           VALUES ($1, 'mean_reversion', $2)
           RETURNING strategy_id"#,
        uid,
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


#[post("/trend_follow")]
async fn start_trend_follow(
    req:  HttpRequest,
    db: web::Data<PgPool>,
    body: web::Json<serde_json::Value>,
) -> impl Responder {
    let uid = match user_id(&req) { Ok(v) => v, Err(e) => return e };

    let row = sqlx::query!(
        r#"INSERT INTO user_strategies (user_id, name, params)
           VALUES ($1,'trend_follow',$2) RETURNING strategy_id"#,
        uid,
        body.0
    )
        .fetch_one(db.as_ref())
        .await;

    match row {
        Ok(r) => HttpResponse::Ok().json(ApiResponse::ok(r.strategy_id)),
        Err(e)=> { log::error!("DB error: {e}");
            HttpResponse::InternalServerError()
                .json(ApiResponse::<()>::err("db error")) }
    }
}

#[post("/vcsr")]
async fn start_vcsr(
    req:  HttpRequest,
    db:  web::Data<PgPool>,
    body: web::Json<serde_json::Value>,
) -> impl Responder {
    let uid = match user_id(&req) { Ok(v) => v, Err(e) => return e };

    let row = sqlx::query!(
        r#"INSERT INTO user_strategies (user_id, name, params)
           VALUES ($1, 'vcsr', $2) RETURNING strategy_id"#,
        uid,
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
    req:  HttpRequest,
    db: web::Data<PgPool>,
    path: web::Path<Uuid>,
) -> impl Responder {
    let uid = match user_id(&req) { Ok(v) => v, Err(e) => return e };
    let id  = *path;

    let result = sqlx::query!(
        r#"
        UPDATE user_strategies
           SET status = 'disabled'
         WHERE strategy_id = $1
           AND user_id     = $2
        "#,
        id,
        uid
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
    req: HttpRequest,
    db: web::Data<PgPool>,
) -> impl Responder {
    let uid = match user_id(&req) { Ok(v) => v, Err(e) => return e };

    let rows = sqlx::query_as!(
        UserStrategy,
        r#"SELECT strategy_id, user_id, name, params, status, created_at
            FROM user_strategies
            WHERE user_id = $1
              AND status  = 'enabled'
        "#,
        uid
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
        .service(start_trend_follow)
        .service(start_vcsr)
        .service(stop_strategy)
        .service(list_active)
}