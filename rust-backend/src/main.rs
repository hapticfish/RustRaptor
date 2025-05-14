
use actix_web::{middleware::Logger, web, App, HttpServer};
use actix_web::cookie::time::Duration;
use sqlx::postgres::PgPoolOptions;
use tokio;

use rustraptor_backend::{
    config::settings::Settings,
    services::strategies::mean_reversion,
    services::scheduler,
    db::redis::RedisPool,
    routes::{
        health::health_scope,
        trading::trading_scope,
        copy::copy_scope,
        strategies::strategy_scope,
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

    let pg_pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&settings.database_url)
        .await
        .expect("postgres");

    let redis_pool = RedisPool::new(&settings.redis_url)
        .await
        .expect("redis");

    // --- scheduler reconciler ----------------------------------------------
    {
        let pg     = pg_pool.clone();
        let redis  = redis_pool.clone();
        let s_copy = settings.clone();
        tokio::spawn(async move {
            let mut iv = tokio::time::interval(std::time::Duration::from_secs(30));
            loop {
                iv.tick().await;
                if let Err(e) = scheduler::reconcile(&pg, &redis, &s_copy).await {
                    log::error!("scheduler: {e:?}");
                }
            }
        });
    }


    HttpServer::new(move || {
        App::new()
            .wrap(Logger::default())
            .app_data(web::Data::new(settings_clone.clone()))
            .app_data(web::Data::new(pg_pool.clone()))
            .app_data(web::Data::new(redis_pool.clone()))

            //scope
            .service(health_scope())
            .service(trading_scope())
            .service(copy_scope())
            .service(strategy_scope())

            //degug
            .service(dump_routes)
            .service(request_info)
            .service(param_test)

    })
        .bind(("0.0.0.0", port))?
        .run()
        .await
}

/*todo
   find away to implement early close triggers for failed moves for each stratagies (premium feature)


    for all stratagies review indetail with reasearch and refine the for bgest performance

    adding customizable peramters as needed
   */