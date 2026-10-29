use axum::{extract::State, Json};
use serde_json::json;

use super::super::{
    dto::{
        ApiResponse, ExtraTimeRequest, ManualActionKind, ManualActionRequest, ToggleControlRequest,
    },
    util::{apply_extra, extra_response, resolve_manual_until, set_internet_blocked},
    AppState,
};
use crate::{
    error::{AppError, AppResult},
    log_error, log_info,
    policy::do_sync_user_access,
    users::get_active_user_sid,
    AdminCommand,
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

        crate::activity::log_activity_now(&state.db, &sid, "mode_changed", "auto");
        return Ok(Json(ApiResponse {
            success: true,
            message: "🤖 Режим 'По расписанию'".to_string(),
        }));
    }

    let manual_until = resolve_manual_until(&req.until)
        .ok_or_else(|| AppError::BadRequest("invalid until".into()))?;

    let (mode, cmd, action_str) = match req.action {
        ManualActionKind::Lock => ("manual_lock", AdminCommand::ForceLock, "🔒 Заблокировано"),
        ManualActionKind::Unlock => (
            "manual_unlock",
            AdminCommand::ForceUnlock,
            "🔓 Разблокировано",
        ),
        ManualActionKind::Auto => unreachable!(),
    };

    log_info!("Пишем новый mode: {:?}", &manual_until);
    state.db.set_today_mode(&sid, mode, &manual_until)?;
    state
        .admin_cmd_tx
        .send(cmd)
        .await
        .map_err(|e| AppError::Internal(format!("send cmd failed: {e}")))?;

    let event_type = match req.action {
        ManualActionKind::Lock => "manual_lock",
        ManualActionKind::Unlock => "manual_unlock",
        ManualActionKind::Auto => "mode_changed",
    };
    let details = if manual_until.is_empty() {
        "".to_string()
    } else {
        format!("until: {}", manual_until)
    };

    // 🆕 Заменяем старый ручной log_activity на вызов нашего хелпера
    crate::activity::log_activity_now(&state.db, &sid, event_type, &details);

    let msg = if manual_until.is_empty() {
        action_str.to_string()
    } else {
        format!("{action_str} до {manual_until}")
    };
    Ok(Json(ApiResponse {
        success: true,
        message: msg,
    }))
}

pub async fn extra_time(
    State(state): State<AppState>,
    Json(req): Json<ExtraTimeRequest>,
) -> AppResult<Json<serde_json::Value>> {
    if req.sid.is_empty() {
        return Err(AppError::BadRequest("sid is required".into()));
    }
    apply_extra(
        &state.db,
        &req.sid,
        req.screen_minutes,
        req.internet_minutes,
        true,
    )?;
    if let Err(e) = do_sync_user_access(&state.db, &state.admin_cmd_tx, &req.sid).await {
        log_error!("[extra_time] sync failed: {:?}", e);
    }
    let details = format!(
        "screen: {:?}m, internet: {:?}m",
        req.screen_minutes, req.internet_minutes
    );

    // 🆕 Заменяем старый ручной log_activity
    crate::activity::log_activity_now(&state.db, &req.sid, "extra_time", &details);

    Ok(Json(extra_response(
        &req.sid,
        req.screen_minutes,
        req.internet_minutes,
    )))
}

pub async fn set_extra_time(
    State(state): State<AppState>,
    Json(req): Json<ExtraTimeRequest>,
) -> AppResult<Json<serde_json::Value>> {
    if req.sid.is_empty() {
        return Err(AppError::BadRequest("sid is required".into()));
    }
    apply_extra(
        &state.db,
        &req.sid,
        req.screen_minutes,
        req.internet_minutes,
        false,
    )?;
    if let Err(e) = do_sync_user_access(&state.db, &state.admin_cmd_tx, &req.sid).await {
        log_error!("[set_extra_time] sync failed: {:?}", e);
    }
    let details = format!(
        "screen: {:?}m, internet: {:?}m",
        req.screen_minutes, req.internet_minutes
    );

    // 🆕 Заменяем старый ручной log_activity
    crate::activity::log_activity_now(&state.db, &req.sid, "set_extra_time", &details);

    Ok(Json(extra_response(
        &req.sid,
        req.screen_minutes,
        req.internet_minutes,
    )))
}

pub async fn toggle_user_control(
    State(state): State<AppState>,
    Json(req): Json<ToggleControlRequest>,
) -> AppResult<Json<serde_json::Value>> {
    state.db.set_user_control_enabled(&req.sid, req.enabled)?;
    log_info!(
        "🔄 [API] Контроль для {} {}",
        req.sid,
        if req.enabled {
            "ВКЛЮЧЕН"
        } else {
            "ВЫКЛЮЧЕН"
        }
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

    // 🆕 Логируем смену режима контроля, чтобы на графике сразу изменился цвет
    let details = if req.enabled { "enabled" } else { "disabled" };
    crate::activity::log_activity_now(&state.db, &req.sid, "control_toggled", details);

    Ok(Json(json!({ "success": true })))
}

pub async fn pause_internet(State(state): State<AppState>) -> AppResult<Json<ApiResponse>> {
    let Some(sid) = get_active_user_sid() else {
        return Err(AppError::BadRequest("no active user".into()));
    };
    set_internet_blocked(&state.db, &sid, true, "manual")?;
    crate::activity::log_activity_now(&state.db, &sid, "internet_blocked", "manual");
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
    crate::activity::log_activity_now(&state.db, &sid, "internet_unblocked", "manual");
    Ok(Json(ApiResponse {
        success: true,
        message: "🌐 Интернет возобновлён".to_string(),
    }))
}

pub async fn internet_status(State(state): State<AppState>) -> AppResult<Json<serde_json::Value>> {
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
