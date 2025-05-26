pub mod config;
pub mod db;
pub mod middleware;
pub mod routes {
    pub mod copy;
    pub mod health;
    pub mod trading;
    pub mod strategies;
}
pub mod services {
    pub mod scheduler;
    pub mod trading_engine;
    pub mod market_data;

    pub mod risk;

    pub mod blowfin;
    pub mod copy_trading;
    pub mod strategies {
        pub mod common;
        pub use common::{Candle, OrderBookSnapshot};
        pub mod mean_reversion;
        pub mod trend_follow;
        pub mod vcsr;
    }
}

pub mod utils;

