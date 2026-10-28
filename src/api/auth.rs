use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    middleware::Next,
    response::Response,
};

use super::AppState;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum AuthLevel {
    None,
    Tray,
    Admin,
}

/// Требует хотя бы Tray. Возвращает 401, если токен не подошёл.
pub async fn require_auth(
    State(state): State<AppState>,
    headers: HeaderMap,
    req: axum::extract::Request,
    next: Next,
) -> Result<Response, StatusCode> {
    if check_auth_level(&headers, &state) == AuthLevel::None {
        return Err(StatusCode::UNAUTHORIZED);
    }
    Ok(next.run(req).await)
}

/// Требует Admin (api_token или admin_pin).
pub async fn require_admin(
    State(state): State<AppState>,
    headers: HeaderMap,
    req: axum::extract::Request,
    next: Next,
) -> Result<Response, StatusCode> {
    if check_auth_level(&headers, &state) < AuthLevel::Admin {
        return Err(StatusCode::UNAUTHORIZED);
    }
    Ok(next.run(req).await)
}

pub fn check_auth_level(headers: &HeaderMap, state: &AppState) -> AuthLevel {
    let Some(bearer) = headers
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
    else {
        return AuthLevel::None;
    };

    if bearer == state.api_token {
        return AuthLevel::Admin;
    }

    if let Ok(Some(pin)) = state.db.get_setting("admin_pin") {
        if !pin.is_empty() && bearer == pin {
            return AuthLevel::Admin;
        }
    }

    if let Ok(Some(tray)) = state.db.get_setting("tray_token") {
        if !tray.is_empty() && bearer == tray {
            return AuthLevel::Tray;
        }
    }

    AuthLevel::None
}

