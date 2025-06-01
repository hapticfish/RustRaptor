// src/routes/health.rs
use actix_web::{get, web, HttpResponse, Scope};

#[get("")]
async fn health_check() -> HttpResponse {
    HttpResponse::Ok().body("OK")
}

pub fn health_scope() -> Scope {
    web::scope("/health").service(health_check)
}
