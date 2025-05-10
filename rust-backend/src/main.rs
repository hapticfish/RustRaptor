
use actix_web::{middleware::Logger, web, App, HttpServer};
use sqlx::postgres::PgPoolOptions;

use rustraptor_backend::config::settings::Settings;
use rustraptor_backend::routes::health::health_scope;
use rustraptor_backend::routes::trading::trading_scope;
use rustraptor_backend::utils::route_debug::{dump_routes, request_info, param_test};

fn init_logging() {
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("info"),
    )
        .init();
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    init_logging();
    println!("Starting RustRaptor backend…");

    let settings = Settings::new().unwrap_or_else(|e| {
        eprintln!("Failed to load settings: {e}");
        std::process::exit(1);
    });

    let port = settings.server_port;
    let settings_clone = settings.clone();

    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&settings.database_url)
        .await
        .expect("Failed to open Postgres pool");

    HttpServer::new(move || {
        App::new()
            .wrap(Logger::default())
            .app_data(web::Data::new(settings_clone.clone()))
            .app_data(web::Data::new(pool.clone()))
            .service(health_scope())
            .service(trading_scope())

            .service(dump_routes)
            .service(request_info)
            .service(param_test)

    })
        .bind(("0.0.0.0", port))?
        .run()
        .await
}