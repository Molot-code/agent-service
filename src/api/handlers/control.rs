use axum::{extract::State, Json};
use serde_json::json;

use crate::{
    error::{AppError, AppResult},
    log_error, log_info,
    policy::do_sync_user_access,
    users::get_active_user_sid,
    AdminCommand,
};

use super::super::{
    dto::{ApiResponse, ExtraTimeRequest, ManualActionKind, ManualActionRequest, ToggleControlRequest},
    util::{apply_extra, extra_response, resolve_manual_until, set_internet_blocked},
    AppState,
};

pub async fn manual_action(
    State(state): State<AppState>,
    Json(req): Json<ManualActionRequest>,
) -> AppResult<Json<ApiResponse>> {
    let Some(sid) = get_active_user_sid() else {
        return Err(AppError::BadRequest("no active user".into()));
    };

    if matches!(req.action, ManualActionKind::Auto) {
        state.db.set_today_mode(&sid, "scheduled", "")?;
        if let Err(e) = do_sync_user_access(&state.db, &state.admin_cmd_tx, &sid).await {
            log_error!("[API manual_action] sync failed: {:?}", e);
        }
        state
            .admin_cmd_tx
            .send(AdminCommand::AutoMode)
            .await
            .map_err(|e| AppError::Internal(format!("send cmd failed: {e}")))?;
        let (screen_time, internet_time) = state.db.get_today_usage(&sid).unwrap_or((0, 0));
        let today_limit = state.db.ensure_today_limit(&sid).unwrap_or_default();
        let _ = state.db.log_activity(&sid, "mode_changed", "auto", screen_time,
            internet_time, today_limit.limit_sec, today_limit.internet_limit_sec,
            false, false);
        return Ok(Json(ApiResponse {
            success: true,
            message: "🤖 Режим 'По расписанию'".to_string(),
        }));
    }

    let manual_until = resolve_manual_until(&req.until)
        .ok_or_else(|| AppError::BadRequest("invalid until".into()))?;

    let (mode, cmd, action_str) = match req.action {
        ManualActionKind::Lock => ("manual_lock", AdminCommand::ForceLock, "🔒 Заблокировано"),
        ManualActionKind::Unlock => {
            ("manual_unlock", AdminCommand::ForceUnlock, "🔓 Разблокировано")
        }
        ManualActionKind::Auto => unreachable!(),
    };

    log_info!("Пишем новый mode: {:?}", &manual_until);
    state.db.set_today_mode(&sid, mode, &manual_until)?;
    state
        .admin_cmd_tx
        .send(cmd)
        .await
        .map_err(|e| AppError::Internal(format!("send cmd failed: {e}")))?;
    let (screen_time, internet_time) = state.db.get_today_usage(&sid).unwrap_or((0, 0));
    let today_limit = state.db.ensure_today_limit(&sid).unwrap_or_default();
    let event_type = match req.action {
        ManualActionKind::Lock => "manual_lock",
        ManualActionKind::Unlock => "manual_unlock",
        ManualActionKind::Auto => "mode_changed",
    };
    let details = if manual_until.is_empty() { "".to_string() } else { format!("until: {}", manual_until) };
    let _ = state.db.log_activity(&sid, event_type, &details, screen_time,
        internet_time, today_limit.limit_sec, today_limit.internet_limit_sec,
        matches!(req.action, ManualActionKind::Lock), false);

    let msg = if manual_until.is_empty() {
        action_str.to_string()
    } else {
        format!("{action_str} до {manual_until}")
    };
    Ok(Json(ApiResponse { success: true, message: msg }))
}

pub async fn extra_time(
    State(state): State<AppState>,
    Json(req): Json<ExtraTimeRequest>,
) -> AppResult<Json<serde_json::Value>> {
    if req.sid.is_empty() {
        return Err(AppError::BadRequest("sid is required".into()));
    }
    apply_extra(&state.db, &req.sid, req.screen_minutes, req.internet_minutes, true)?;
    if let Err(e) = do_sync_user_access(&state.db, &state.admin_cmd_tx, &req.sid).await {
        log_error!("[extra_time] sync failed: {:?}", e);
    }
    let (screen_time, internet_time) = state.db.get_today_usage(&req.sid).unwrap_or((0, 0));
    let today_limit = state.db.ensure_today_limit(&req.sid).unwrap_or_default();
    let details = format!("screen: {:?}m, internet: {:?}m", req.screen_minutes, req.internet_minutes);
    let _ = state.db.log_activity(&req.sid, "extra_time", &details, screen_time,
        internet_time, today_limit.limit_sec, today_limit.internet_limit_sec,
        false, false);
    Ok(Json(extra_response(&req.sid, req.screen_minutes, req.internet_minutes)))
}

pub async fn set_extra_time(
    State(state): State<AppState>,
    Json(req): Json<ExtraTimeRequest>,
) -> AppResult<Json<serde_json::Value>> {
    if req.sid.is_empty() {
        return Err(AppError::BadRequest("sid is required".into()));
    }
    apply_extra(&state.db, &req.sid, req.screen_minutes, req.internet_minutes, false)?;
    if let Err(e) = do_sync_user_access(&state.db, &state.admin_cmd_tx, &req.sid).await {
        log_error!("[set_extra_time] sync failed: {:?}", e);
    }
    let (screen_time, internet_time) = state.db.get_today_usage(&req.sid).unwrap_or((0, 0));
    let today_limit = state.db.ensure_today_limit(&req.sid).unwrap_or_default();
    let details = format!("screen: {:?}m, internet: {:?}m", req.screen_minutes, req.internet_minutes);
    let _ = state.db.log_activity(&req.sid, "set_extra_time", &details, screen_time,
        internet_time, today_limit.limit_sec, today_limit.internet_limit_sec,
        false, false);
    Ok(Json(extra_response(&req.sid, req.screen_minutes, req.internet_minutes)))
}

pub async fn toggle_user_control(
    State(state): State<AppState>,
    Json(req): Json<ToggleControlRequest>,
) -> AppResult<Json<serde_json::Value>> {
    state.db.set_user_control_enabled(&req.sid, req.enabled)?;
    log_info!(
        "🔄 [API] Контроль для {} {}",
        req.sid,
        if req.enabled { "ВКЛЮЧЕН" } else { "ВЫКЛЮЧЕН" }
    );

    if !req.enabled {
        if let Some(active) = get_active_user_sid() {
            if active == req.sid {
                log_info!("⚡ [API] Активный пользователь заблокирован — снимаем lock_status");
                let _ = state.db.set_state("lock_status", "false");
            }
        }
    }
    if let Err(e) = do_sync_user_access(&state.db, &state.admin_cmd_tx, &req.sid).await {
        log_error!("[toggle_user_control] sync failed: {:?}", e);
    }
    Ok(Json(json!({ "success": true })))
}

pub async fn pause_internet(State(state): State<AppState>) -> AppResult<Json<ApiResponse>> {
    let Some(sid) = get_active_user_sid() else {
        return Err(AppError::BadRequest("no active user".into()));
    };
    set_internet_blocked(&state.db, &sid, true, "manual")?;
    let (screen_time, internet_time) = state.db.get_today_usage(&sid).unwrap_or((0, 0));
    let today_limit = state.db.ensure_today_limit(&sid).unwrap_or_default();
    let _ = state.db.log_activity(&sid, "internet_blocked", "manual", screen_time,
        internet_time, today_limit.limit_sec, today_limit.internet_limit_sec,
        false, true);
    Ok(Json(ApiResponse {
        success: true,
        message: "🌐 Интернет на паузе".to_string(),
    }))
}

pub async fn resume_internet(State(state): State<AppState>) -> AppResult<Json<ApiResponse>> {
    let Some(sid) = get_active_user_sid() else {
        return Err(AppError::BadRequest("no active user".into()));
    };
    set_internet_blocked(&state.db, &sid, false, "")?;
    let (screen_time, internet_time) = state.db.get_today_usage(&sid).unwrap_or((0, 0));
    let today_limit = state.db.ensure_today_limit(&sid).unwrap_or_default();
    let _ = state.db.log_activity(&sid, "internet_unblocked", "manual", screen_time,
        internet_time, today_limit.limit_sec, today_limit.internet_limit_sec,
        false, false);
    Ok(Json(ApiResponse {
        success: true,
        message: "🌐 Интернет возобновлён".to_string(),
    }))
}

pub async fn internet_status(
    State(state): State<AppState>,
) -> AppResult<Json<serde_json::Value>> {
    let Some(sid) = get_active_user_sid() else {
        return Ok(Json(json!({ "paused": false, "blocked": false })));
    };
    let paused = state.db.is_internet_paused(&sid).unwrap_or(false);
    let blocked = state
        .db
        .get_state("internet_blocked")
        .ok()
        .flatten()
        .as_deref()
        == Some("true");
    Ok(Json(json!({ "paused": paused, "blocked": blocked })))
}