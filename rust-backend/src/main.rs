use tracing_subscriber::{fmt, EnvFilter};
use actix_web::{middleware::Logger, web, App, HttpServer};
use metrics_exporter_prometheus::PrometheusBuilder;
use rustraptor_backend::services::risk;
use sqlx::postgres::PgPoolOptions;

use rustraptor_backend::{
    config::settings::Settings,
    db::redis::RedisPool,
    routes::{
        copy::copy_scope, health::health_scope, strategies::strategy_scope, trading::trading_scope,
    },
    services,
    services::scheduler,
    utils::route_debug::{dump_routes, param_test, request_info},
};
use rustraptor_backend::middleware::metrics::Metrics;

// fn init_logging() {
//     env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
// }

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    fmt::Subscriber::builder().with_env_filter(EnvFilter::from_default_env()).json().init();

    PrometheusBuilder::new()
        .with_http_listener(([0, 0, 0, 0], 9000))
        .install()
        .expect("metrics exporter");

    // init_logging();
    println!("Starting RustRaptor backend…");

    let settings = Settings::new().unwrap_or_else(|e| {
        eprintln!("Failed to load settings: {e}");
        std::process::exit(1);
    });

    println!("Connecting to database: {}", &settings.database_url);

    let bus = services::market_data::spawn_all_feeds(&settings).await;
    let port = settings.server_port;
    let settings_clone = settings.clone();

    let pg_pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&settings.database_url)
        .await
        .expect("postgres");

    let redis_pool = RedisPool::new(&settings.redis_url).await.expect("redis");

    risk::spawn_guardian(pg_pool.clone(), redis_pool.clone());

    // --- scheduler reconciler ----------------------------------------------
    {
        let pg = pg_pool.clone();
        let redis = redis_pool.clone();
        let s_copy = settings.clone();
        let bus_c = bus.clone();
        tokio::spawn(async move {
            let mut iv = tokio::time::interval(std::time::Duration::from_secs(30));
            loop {
                iv.tick().await;
                if let Err(e) = scheduler::reconcile(&pg, &redis, &s_copy, &bus_c).await {
                    log::error!("scheduler: {e:?}");
                }
            }
        });
    }

    HttpServer::new(move || {
        App::new()
            .wrap(Metrics)
            .wrap(Logger::default())
            .wrap(rustraptor_backend::middleware::Auth)
            .app_data(web::Data::new(settings_clone.clone()))
            .app_data(web::Data::new(pg_pool.clone()))
            .app_data(web::Data::new(redis_pool.clone()))
            .app_data(web::Data::new(bus.clone()))
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
