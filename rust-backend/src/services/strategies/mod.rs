pub mod common;
pub use common::{Candle, OrderBookSnapshot};
pub mod mean_reversion;
pub mod trend_follow;
pub mod vcsr;

pub use vcsr::VcsrStrategy;