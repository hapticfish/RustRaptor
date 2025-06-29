//! ──────────────────────────────────────────────────────────────────────────
//! Per-user risk limits
//! ──────────────────────────────────────────────────────────────────────────
//! * Slippage guard  – checked synchronously per order
//! * Draw-down guard – rolling 24 h realised PnL window (Redis)
//! * Guardian loop   – background monitor for all active users
//!
//! All limits are hard-coded; later you can persist them in Postgres.
//! ──────────────────────────────────────────────────────────────────────────

use chrono::Utc;
use redis::AsyncCommands;
use sqlx::PgPool;
use tokio::time::{interval, Duration};

use crate::{db::redis::RedisPool, utils::errors::TradeError};

/// ─── Constants ───────────────────────────────────────────────────────────
const MAX_SLIPPAGE_BPS: f64 = 10.0; // 0.10 %
const MAX_DD_PCT: f64 = 20.0; // −20 % over look-back
const LOOKBACK_SECS: i64 = 86_400; // 24 h
const REDIS_TTL: usize = (LOOKBACK_SECS as usize) + 600; // keep a bit longer

/// ─── Public helpers ──────────────────────────────────────────────────────
/// Pre-trade slippage guard (caller passes their own estimate)
#[inline]
pub fn check_slippage(estimated_bps: f64) -> Result<(), TradeError> {
    if estimated_bps > MAX_SLIPPAGE_BPS {
        Err(TradeError::RiskViolation(format!(
            "slippage {:.2} bps exceeds {:.1} bps limit",
            estimated_bps, MAX_SLIPPAGE_BPS
        )))
    } else {
        Ok(())
    }
}

/// Store every fill’s realised PnL in a rolling Redis list
pub async fn record_fill(
    redis: &RedisPool,
    user_id: i64,
    realised_pnl_usd: f64,
) -> redis::RedisResult<()> {
    let key = redis.with_prefix("dd", user_id.to_string());
    let mut conn = redis.manager().as_ref().clone();
    let entry = format!("{}|{:.8}", Utc::now().timestamp(), realised_pnl_usd);
    conn.lpush::<_, _, ()>(&key, entry).await?;
    conn.expire::<_, ()>(&key, REDIS_TTL as i64).await?;
    Ok(())
}

/// Check the 24 h realised PnL window and error on breach
pub async fn check_drawdown(redis: &RedisPool, user_id: i64) -> Result<(), TradeError> {
    let key = redis.with_prefix("dd", user_id.to_string());
    let mut conn = redis.manager().as_ref().clone();
    let rows: Vec<String> = conn.lrange(&key, 0, -1).await.unwrap_or_default();

    let cutoff = Utc::now().timestamp() - LOOKBACK_SECS;
    let dd: f64 = rows
        .into_iter()
        .filter_map(|s| {
            let mut it = s.split('|');
            let ts = it.next()?.parse::<i64>().ok()?;
            let pnl = it.next()?.parse::<f64>().ok()?;
            (ts >= cutoff).then_some(pnl)
        })
        .sum();

    // We assume equity = 100 (you’ll likely replace with real equity later)
    if dd < 0.0 && (-dd) > MAX_DD_PCT {
        Err(TradeError::RiskViolation(format!(
            "draw-down {:.2}% exceeds {:.1}% limit",
            -dd, MAX_DD_PCT
        )))
    } else {
        Ok(())
    }
}

/// ─── Guardian loop ───────────────────────────────────────────────────────
/// Runs in the background, polls the DB every minute, applies draw-down check
pub fn spawn_guardian(pg: PgPool, redis: RedisPool) {
    tokio::spawn(async move {
        let mut iv = interval(Duration::from_secs(60));

        loop {
            iv.tick().await;

            if let Ok(user_ids) = active_users(&pg).await {
                for uid in user_ids {
                    if let Err(e) = check_drawdown(&redis, uid).await {
                        log::warn!("risk DD trip for user {uid}: {e}");
                        // Future: flip a Redis “tripped” flag → strategies can abort early
                    }
                }
            }
        }
    });
}

/// Query distinct user IDs that still have **enabled** strategies
async fn active_users(pg: &PgPool) -> sqlx::Result<Vec<i64>> {
    let rows = sqlx::query! {
        r#"
        SELECT DISTINCT user_id
          FROM user_strategies
         WHERE status = 'enabled'
        "#
    }
    .fetch_all(pg)
    .await?;

    Ok(rows.into_iter().map(|r| r.user_id).collect())
}

// ======================================================================
// UNIT TESTS
// ======================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn compute_dd(rows: &[String], cutoff_ts: i64) -> f64 {
        rows.iter()
            .filter_map(|s| {
                let mut it = s.split('|');
                let ts = it.next()?.parse::<i64>().ok()?;
                let pnl = it.next()?.parse::<f64>().ok()?;
                (ts >= cutoff_ts).then_some(pnl)
            })
            .sum()
    }

    // ───────────────────────────────────────── Slippage guard
    #[test]
    fn slippage_within_limit_passes() {
        assert!(check_slippage(9.99).is_ok());
    }

    #[test]
    fn slippage_at_limit_passes() {
        assert!(check_slippage(MAX_SLIPPAGE_BPS).is_ok());
    }

    #[test]
    fn slippage_over_limit_fails() {
        let e = check_slippage(MAX_SLIPPAGE_BPS + 0.01).unwrap_err();
        match e {
            TradeError::RiskViolation(msg) => assert!(msg.contains("slippage")),
            _ => panic!("wrong error variant"),
        }
    }

    // ───────────────────────────────────────── Draw-down maths helper
    fn make_row(ts: i64, pnl: f64) -> String {
        format!("{}|{:.4}", ts, pnl)
    }

    #[test]
    fn dd_empty_is_zero() {
        let rows: Vec<String> = vec![];
        let sum = compute_dd(&rows, 0);
        assert_eq!(sum, 0.0);
    }

    #[test]
    fn dd_ignores_older_than_cutoff() {
        let now = Utc::now().timestamp();
        let old = now - LOOKBACK_SECS - 10;
        let rows = vec![make_row(old, -5.0), make_row(now, -3.0)];
        let dd = compute_dd(&rows, now - LOOKBACK_SECS);
        assert!((dd + 3.0).abs() < 1e-9);
    }

    #[test]
    fn dd_breach_detected() {
        let now = Utc::now().timestamp();
        let dd = -MAX_DD_PCT - 1.0;
        let rows = vec![make_row(now, dd)];
        let sum = compute_dd(&rows, now - LOOKBACK_SECS);
        assert_eq!(sum, dd);
        // emulate real check
        let e = if sum < 0.0 && (-sum) > MAX_DD_PCT {
            Some(TradeError::RiskViolation("breach".into()))
        } else {
            None
        };
        assert!(e.is_some(), "breach should be flagged");
    }

    #[test]
    fn dd_borderline_allows_trade() {
        let now = Utc::now().timestamp();
        let dd = -MAX_DD_PCT + 0.0001;
        let rows = vec![make_row(now, dd)];
        let sum = compute_dd(&rows, now - LOOKBACK_SECS);
        assert!((-sum) < MAX_DD_PCT);
    }

    #[test]
    fn dd_skips_malformed_rows() {
        let now = Utc::now().timestamp();
        let rows = vec![
            "bad|row".to_string(),
            make_row(now, -1.0),
            "123456".to_string(), // missing pnl
        ];
        let sum = compute_dd(&rows, now - LOOKBACK_SECS);
        assert_eq!(sum, -1.0);
    }
}
