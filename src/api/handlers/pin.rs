use axum::{extract::State, Json};
use serde_json::json;

use crate::error::AppResult;

use super::super::{
    dto::{ApiResponse, CheckPinRequest, SetPinRequest},
    AppState,
};

pub async fn set_admin_pin(
    State(state): State<AppState>,
    Json(payload): Json<SetPinRequest>,
) -> AppResult<Json<ApiResponse>> {
    if let Some(old_pin) = payload.old_pin {
        let stored = state.db.get_setting("admin_pin").ok().flatten().unwrap_or_default();
        if !stored.is_empty() && old_pin != stored {
            return Ok(Json(ApiResponse {
                success: false,
                message: "❌ Неверный старый PIN".to_string(),
            }));
        }
    }

    state.db.set_setting("admin_pin", &payload.new_pin)?;
    Ok(Json(ApiResponse {
        success: true,
        message: "✅ PIN-код изменён".to_string(),
    }))
}

pub async fn check_pin(
    State(state): State<AppState>,
    Json(payload): Json<CheckPinRequest>,
) -> AppResult<Json<ApiResponse>> {
    let stored = state.db.get_setting("admin_pin").ok().flatten().unwrap_or_default();
    let ok = stored.is_empty() || payload.pin == stored;
    Ok(Json(ApiResponse {
        success: ok,
        message: if ok { "✅ Авторизация успешна" } else { "❌ Неверный PIN" }.to_string(),
    }))
}

pub async fn is_pin_set(State(state): State<AppState>) -> AppResult<Json<serde_json::Value>> {
    let pin = state.db.get_setting("admin_pin").ok().flatten().unwrap_or_default();
    Ok(Json(json!({ "is_set": !pin.is_empty() })))
}