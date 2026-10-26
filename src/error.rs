use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("unauthorized")]
    Unauthorized,

    #[error("bad request: {0}")]
    BadRequest(String),

    #[error("not found: {0}")]
    NotFound(String),

    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("internal error: {0}")]
    Internal(String),
}

use crate::{log_debug, log_error, log_info, log_warn};
pub type AppResult<T> = Result<T, AppError>;

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, client_message) = match &self {
            AppError::Unauthorized => (StatusCode::UNAUTHORIZED, "unauthorized".to_string()),
            AppError::BadRequest(m) => (StatusCode::BAD_REQUEST, m.clone()),
            AppError::NotFound(m) => (StatusCode::NOT_FOUND, m.clone()),
            AppError::Database(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal error".to_string(),
            ),
            AppError::Io(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal error".to_string(),
            ),
            AppError::Internal(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal error".to_string(),
            ),
        };

        // Логируем через макросы — БЕЗ format!, он внутри макроса
        match status {
            StatusCode::INTERNAL_SERVER_ERROR => {
                log_error!("[API] {} → {}", status.as_u16(), self);
            }
            StatusCode::UNAUTHORIZED | StatusCode::BAD_REQUEST | StatusCode::NOT_FOUND => {
                log_warn!("[API] {} → {}", status.as_u16(), self);
            }
            _ => {
                log_info!("[API] {} → {}", status.as_u16(), self);
            }
        }

        let body = Json(json!({
            "success": false,
            "error": client_message,
        }));

        (status, body).into_response()
    }
}
