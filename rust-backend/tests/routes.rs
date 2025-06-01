// tests/routes.rs
use actix_web::{test, web, App, HttpResponse};
use rustraptor_backend::routes::health::health_scope;
// Import the individual handlers directly
use rustraptor_backend::routes::trading::{
    balance, list_routes, simple_test, test_trade_api, trade,
};
use rustraptor_backend::utils::route_debug::{dump_routes, param_test, request_info};

#[actix_rt::test]
async fn test_all_routes_exist() {
    println!("Starting test_all_routes_exist with controlled registration");

    // Start with just API routes
    let app = test::init_service(
        App::new().service(
            web::scope("/api")
                .service(simple_test)
                .service(test_trade_api)
                .service(balance)
                .service(trade)
                .service(list_routes),
        ),
    )
    .await;

    println!("API-only app initialized");

    // Test API routes - these should work
    println!("Testing /api/test with API-only routes");
    let req = test::TestRequest::get().uri("/api/test").to_request();
    let resp = test::call_service(&app, req).await;
    println!("API-only /api/test response: {}", resp.status());
    assert_eq!(resp.status(), 200);

    // Now add health scope and test again
    println!("Now testing with health_scope added");
    let app = test::init_service(
        App::new().service(health_scope()).service(
            web::scope("/api")
                .service(simple_test)
                .service(test_trade_api)
                .service(balance)
                .service(trade)
                .service(list_routes),
        ),
    )
    .await;

    println!("API+health app initialized");

    println!("Testing /api/test with API+health");
    let req = test::TestRequest::get().uri("/api/test").to_request();
    let resp = test::call_service(&app, req).await;
    println!("API+health /api/test response: {}", resp.status());
    assert_eq!(resp.status(), 200);

    // Finally add debug routes
    println!("Now testing with debug routes added");
    let app = test::init_service(
        App::new()
            .service(health_scope())
            .service(
                web::scope("/api")
                    .service(simple_test)
                    .service(test_trade_api)
                    .service(balance)
                    .service(trade)
                    .service(list_routes),
            )
            .service(dump_routes)
            .service(request_info)
            .service(param_test),
    )
    .await;

    println!("API+health+debug app initialized");

    println!("Testing /api/test with API+health+debug");
    let req = test::TestRequest::get().uri("/api/test").to_request();
    let resp = test::call_service(&app, req).await;
    println!("API+health+debug /api/test response: {}", resp.status());
    assert_eq!(resp.status(), 200);
}

#[actix_rt::test]
async fn test_route_debug_implementation() {
    use rustraptor_backend::utils::route_debug::{dump_routes, param_test, request_info};

    // Check what routes are registered
    println!("Testing route_debug implementation");

    // Register just the debug routes
    let app = test::init_service(
        App::new()
            .service(dump_routes)
            .service(request_info)
            .service(param_test),
    )
    .await;

    // Test a few paths to see what's registered
    for path in ["/debug/routes", "/debug/param/42", "/api/test"] {
        println!("Testing path with debug routes only: {}", path);
        let req = test::TestRequest::get().uri(path).to_request();
        let resp = test::call_service(&app, req).await;
        println!("Debug-only response for {}: {}", path, resp.status());
    }
}

#[actix_rt::test]
async fn test_direct_registration() {
    use rustraptor_backend::routes::trading::{
        balance, list_routes, simple_test, test_trade_api, trade,
    };

    println!("Starting test_direct_registration");

    // Register routes directly without going through trading_scope()
    let app = test::init_service(
        App::new().service(
            web::scope("/api")
                .service(simple_test)
                .service(test_trade_api)
                .service(balance)
                .service(trade)
                .service(list_routes),
        ),
    )
    .await;

    println!("Application with direct registration initialized");

    // Test the routes
    println!("Testing directly registered /api/simple");
    let req = test::TestRequest::get().uri("/api/simple").to_request();
    let resp = test::call_service(&app, req).await;
    println!("Direct registration /api/simple: {}", resp.status());
    assert_eq!(resp.status(), 200);

    println!("Testing directly registered /api/test");
    let req = test::TestRequest::get().uri("/api/test").to_request();
    let resp = test::call_service(&app, req).await;
    println!("Direct registration /api/test: {}", resp.status());
    assert_eq!(resp.status(), 200);
}

// Keeping this additional test can be helpful for debugging
#[actix_rt::test]
async fn test_without_middleware() {
    use rustraptor_backend::routes::trading::{
        balance, list_routes, simple_test, test_trade_api, trade,
    };

    println!("Starting test_without_middleware");

    // Create a test app without using trading_scope() and without middleware
    let app = test::init_service(
        App::new().service(
            web::scope("/api")
                // No middleware here
                .service(simple_test)
                .service(test_trade_api)
                .service(balance)
                .service(trade)
                .service(list_routes),
        ),
    )
    .await;

    println!("Application without middleware initialized");

    println!("Testing /api/test without middleware");
    let req = test::TestRequest::get().uri("/api/test").to_request();
    let resp = test::call_service(&app, req).await;
    println!("No middleware /api/test: {}", resp.status());
    assert_eq!(resp.status(), 200);
}

#[actix_rt::test]
async fn test_minimal_registration() {
    println!("Starting test_minimal_registration");

    // Only register the API routes, nothing else
    let app =
        test::init_service(App::new().service(web::scope("/api").service(test_trade_api))).await;

    println!("Minimal app initialized");

    println!("Testing minimal /api/test");
    let req = test::TestRequest::get().uri("/api/test").to_request();
    let resp = test::call_service(&app, req).await;
    println!("Minimal /api/test response: {}", resp.status());
    assert_eq!(resp.status(), 200);
}

#[actix_rt::test]
async fn test_function_registration() {
    println!("Starting test_function_registration");

    // Register route using function reference
    let app = test::init_service(App::new().service(web::scope("/api").route(
        "/test",
        web::get().to(|| async { HttpResponse::Ok().body("Test function") }),
    )))
    .await;

    println!("Function registration app initialized");

    println!("Testing function /api/test");
    let req = test::TestRequest::get().uri("/api/test").to_request();
    let resp = test::call_service(&app, req).await;
    println!("Function /api/test response: {}", resp.status());
    assert_eq!(resp.status(), 200);
}

#[actix_rt::test]
async fn test_only_api_test_route() {
    use rustraptor_backend::routes::trading::test_trade_api;

    println!("Starting test_only_api_test_route");

    // Register ONLY the /api/test route
    let app =
        test::init_service(App::new().service(web::scope("/api").service(test_trade_api))).await;

    println!("Only /api/test app initialized");

    println!("Testing only /api/test");
    let req = test::TestRequest::get().uri("/api/test").to_request();
    let resp = test::call_service(&app, req).await;
    println!("Only /api/test response: {}", resp.status());
    assert_eq!(resp.status(), 200);
}
