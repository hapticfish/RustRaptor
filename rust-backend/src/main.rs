
use actix_web::{middleware::Logger, web, App, HttpServer};
use sqlx::postgres::PgPoolOptions;
use tokio;

use rustraptor_backend::{
    config::settings::Settings,
    services::strategies::mean_reversion::run_mean_reversion,
    db::redis::RedisPool,
    routes::{
        health::health_scope,
        trading::trading_scope,
        copy::copy_scope,
    },
    utils::route_debug::{dump_routes, request_info, param_test},
};


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

    let redis_pool = RedisPool::new(&settings.redis_url)
        .await
        .expect("Could not connect to Redis");

    // 4) spawn your Bollinger mean-reversion loop
    {
        let redis_for_task = redis_pool.clone();
        let settings_for_task = settings.clone();
        tokio::spawn(async move {
            let mut interval =
                tokio::time::interval(tokio::time::Duration::from_secs(4 * 3600));
            loop {
                interval.tick().await;
                run_mean_reversion(redis_for_task.clone(), settings_for_task.clone()).await;
            }
        });
    }

    HttpServer::new(move || {
        App::new()
            .wrap(Logger::default())
            .app_data(web::Data::new(settings_clone.clone()))
            .app_data(web::Data::new(pool.clone()))
            .app_data(web::Data::new(redis_pool.clone()))

            //scope
            .service(health_scope())
            .service(trading_scope())
            .service(copy_scope())

            //degug
            .service(dump_routes)
            .service(request_info)
            .service(param_test)

    })
        .bind(("0.0.0.0", port))?
        .run()
        .await
}