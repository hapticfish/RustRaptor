use actix_web::{post, delete, web, HttpResponse};
use crate::{
    db::redis::RedisPool,
    services::copy_trading::{add_follower, remove_follower},
};
use sqlx::PgPool;

#[post("/copy/{leader_id}")]
async fn follow(
    path: web::Path<i64>,
    pg:   web::Data<PgPool>,
    redis: web::Data<RedisPool>,
    auth:  actix_web::web::ReqData<i64>,          // (discord user id inserted by auth middleware)
) -> HttpResponse {
    let leader = path.into_inner();
    let follower = *auth;                         // our own id

    match add_follower(&pg, &redis, leader, follower).await {
        Ok(_) => HttpResponse::Ok().body("following"),
        Err(e) => {
            log::warn!("follow failed: {}", e);
            HttpResponse::BadRequest().body(e.to_string())
        }
    }
}

#[delete("/copy/{leader_id}")]
async fn unfollow(
    path: web::Path<i64>,
    pg:   web::Data<PgPool>,
    redis: web::Data<RedisPool>,
    auth:  actix_web::web::ReqData<i64>,
) -> HttpResponse {
    let leader = path.into_inner();
    let follower = *auth;

    match remove_follower(&pg, &redis, leader, follower).await {
        Ok(_)  => HttpResponse::Ok().body("un-followed"),
        Err(e) => {
            log::warn!("unfollow failed: {}", e);
            HttpResponse::BadRequest().body(e.to_string())
        }
    }
}

pub fn copy_scope() -> actix_web::Scope {
    web::scope("/api")        // shares `/api` prefix
        .service(follow)
        .service(unfollow)
}
