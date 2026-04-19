//! Uygulama hata enum'u — `thiserror` tabanlı.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("invalid slug '{slug}': {reason}")]
    InvalidSlug { slug: String, reason: String },

    #[error("bot {bot_id} not found")]
    BotNotFound { bot_id: i64 },

    #[error("missing credentials for bot {bot_id}")]
    MissingCredentials { bot_id: i64 },

    #[error("gamma api: {0}")]
    Gamma(String),

    #[error("clob api: {0}")]
    Clob(String),

    #[error("websocket: {0}")]
    WebSocket(String),

    #[error("auth: {0}")]
    Auth(String),

    #[error("config: {0}")]
    Config(String),

    #[error("strategy: {0}")]
    Strategy(String),

    #[error("io: {0}")]
    Io(#[from] std::io::Error),

    #[error("serde: {0}")]
    Serde(#[from] serde_json::Error),

    #[error("sqlx: {0}")]
    Sqlx(#[from] sqlx::Error),

    #[error("sqlx migrate: {0}")]
    SqlxMigrate(#[from] sqlx::migrate::MigrateError),

    #[error("http: {0}")]
    Http(#[from] reqwest::Error),

    #[error("other: {0}")]
    Other(#[from] anyhow::Error),
}

impl AppError {
    /// HTTP status mapping — `IntoResponse` ve API katmanı tarafından kullanılır.
    /// Domain hataları (4xx) ile beklenmeyen sistem hataları (5xx) ayrılır.
    pub fn status_code(&self) -> StatusCode {
        match self {
            Self::BotNotFound { .. } => StatusCode::NOT_FOUND,
            Self::InvalidSlug { .. } | Self::Config(_) => StatusCode::BAD_REQUEST,
            Self::MissingCredentials { .. } | Self::Auth(_) => StatusCode::UNAUTHORIZED,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let status = self.status_code();
        (status, self.to_string()).into_response()
    }
}
