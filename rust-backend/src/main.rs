mod config;
mod routes;
mod services;
mod db;
mod utils;
mod middleware;

use actix_web::{web, App, HttpServer};
use config::settings::Settings;
use routes::health::health_scope;
use sqlx::postgres::PgPoolOptions;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    println!("Starting RustRaptor backend...");
    let settings = match Settings::new() {
        Ok(s) => s,
        Err(e) => {
            eprintln!(" Failed to load settings: {}", e);
            std::process::exit(1);
        }
    };

    println!(" RustRaptor running on http://0.0.0.0:{}", settings.server_port);

    let database_url = settings.database_url.clone();
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await
        .expect("Failed to create Postgres pool");

    let port = settings.server_port;
    let settings_clone = settings.clone();

    HttpServer::new(move || {
        App::new()
            .app_data(actix_web::web::Data::new(settings_clone.clone()))
            .app_data(web::Data::new(pool.clone()))
            .service(health_scope())
    })
        .bind(("0.0.0.0", port))?
        .run()
        .await
}


/*
todo
     ws.rs - ApiError (Cannot find type `ApiError` in this scope [E0412])
        probably because utils/errors.rs is empty
    trading_engine.rs - TradeError and ApiError same issue

*/