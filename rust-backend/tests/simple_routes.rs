use actix_web::{get, test, web, App, HttpResponse, Responder};

// Define a very simple handler inline
#[get("/hello")]
async fn hello() -> impl Responder {
    HttpResponse::Ok().body("Hello")
}

#[actix_rt::test]
async fn test_basic_routing() {
    // Very simple test with a single scope and handler
    let app = test::init_service(App::new().service(web::scope("/test").service(hello))).await;

    // Test the route
    let req = test::TestRequest::get().uri("/test/hello").to_request();
    println!("Request URI: {:?}", req.uri());
    let resp = test::call_service(&app, req).await;
    println!("Response status: {}", resp.status());
    assert_eq!(resp.status(), 200);
}
