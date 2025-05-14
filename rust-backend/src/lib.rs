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

    pub mod copy_trading;
    pub mod strategies {
        pub mod mean_reversion;
        // trend_follow, vscr …
    }
}

pub mod utils;

