use actix_web::{web, HttpResponse, Responder, get};


#[get("/debug/routes")]
pub async fn dump_routes() -> impl Responder {

    let routes = vec![
        "GET /health",
        "GET /api/test",
        "GET /api/balance",
        "GET /api/routes",
        "GET /api/simple",
        "POST /api/trade",
        "GET /debug/routes", // This route
    ];

    HttpResponse::Ok().json(routes)
}

// Function to log request info - helps debug what's happening in tests
#[get("/debug/request-info/{path:.*}")]
pub async fn request_info(path: web::Path<String>) -> impl Responder {
    let path_param = path.into_inner();

    let info = serde_json::json!({
        "captured_path": path_param,
        "time": chrono::Utc::now().to_string(),
    });

    HttpResponse::Ok().json(info)
}

// Add a parameter extraction test route
#[get("/debug/param/{id}")]
pub async fn param_test(path: web::Path<i32>) -> impl Responder {
    let id = path.into_inner();
    HttpResponse::Ok().body(format!("Parameter test successful. ID: {}", id))
}