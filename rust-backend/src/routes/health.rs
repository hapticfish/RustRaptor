use actix_web::{get, web, HttpResponse, Scope};

#[get("/health")]
async fn health_check() -> HttpResponse {
    HttpResponse::Ok().body("OK")
}

pub fn health_scope() -> Scope {
    web::scope("")
        .service(health_check)
}
