// src/utils/errors.rs

use reqwest;
use serde_json;
use std::{error::Error, fmt};
use tungstenite::Error as WsError;

/// Errors coming from external API calls (HTTP, JSON, WS, etc).
#[derive(Debug)]
pub enum ApiError {
    Http(reqwest::Error),
    Json(serde_json::Error),
    WebSocket(WsError),
    Other(String),
    Custom(String),
}

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ApiError::Http(e) => write!(f, "HTTP error: {}", e),
            ApiError::Json(e) => write!(f, "JSON error: {}", e),
            ApiError::WebSocket(e) => write!(f, "WebSocket error: {}", e),
            ApiError::Other(msg) => write!(f, "{}", msg),
            ApiError::Custom(msg) => write!(f, "Custom error: {}", msg),
        }
    }
}

impl Error for ApiError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            ApiError::Http(e) => Some(e),
            ApiError::Json(e) => Some(e),
            ApiError::WebSocket(e) => Some(e),
            ApiError::Other(_) => None,
            ApiError::Custom(_) => None,
        }
    }
}

// Conversions from underlying errors into ApiError
impl From<reqwest::Error> for ApiError {
    fn from(err: reqwest::Error) -> Self {
        ApiError::Http(err)
    }
}

impl From<sqlx::Error> for ApiError {
    fn from(err: sqlx::Error) -> Self {
        ApiError::Other(format!("Database error: {}", err))
    }
}
impl From<serde_json::Error> for ApiError {
    fn from(err: serde_json::Error) -> Self {
        ApiError::Json(err)
    }
}
impl From<WsError> for ApiError {
    fn from(err: WsError) -> Self {
        ApiError::WebSocket(err)
    }
}

/// Errors at the trading‐engine level: wraps ApiError plus validation issues.
#[derive(Debug)]
pub enum TradeError {
    Api(ApiError),
    InvalidRequest(String),
    Other(String),
    RiskViolation(String),
    MissingKey,
    Db(sqlx::Error),
}

impl fmt::Display for TradeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TradeError::Api(e)           => write!(f, "{e}"),
            TradeError::InvalidRequest(m)
            => write!(f, "Invalid request: {m}"),
            TradeError::RiskViolation(m) => write!(f, "Risk violation: {m}"),
            TradeError::MissingKey       => write!(f, "API key not registered"),
            TradeError::Other(m)         => write!(f, "{m}"),
            TradeError::Db(_) => write!(f, "Database error:"),
        }
    }
}

/// Allow `?` to lift any `ApiError` into the domain layer
impl From<ApiError> for TradeError {
    fn from(e: ApiError) -> Self { TradeError::Api(e) }
}

/// Convenience: lift `sqlx::Error` directly into `TradeError`—
/// lets you keep the plain `?` on async DB calls.
impl From<sqlx::Error> for TradeError {
    fn from(e: sqlx::Error) -> Self { TradeError::Api(e.into()) }
}