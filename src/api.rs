// src/api.rs
use std::{env, path::Path, sync::Arc};

use axum::{
    extract::{Path as AxumPath, Query, State},
    http::{header, HeaderMap},
    response::Html,
    routing::{get, post},
    Json, Router,
};
use minijinja::context;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::sync::mpsc;
use tower_http::services::ServeDir;

use crate::{
    config::Config,
    db::AppDb,
    error::{AppError, AppResult},
    firewall, log_debug, log_error, log_info, log_warn,
    policy::do_sync_user_access,
    types::{ControlMode, DaySchedule, ManualAction, TimeToEvent},
    users::get_active_user_sid,
    AdminCommand,
};

// ===== Модели запросов/ответов =====

#[derive(Serialize)]
pub struct UserResponse {
    pub sid: String,
    pub username: String,
}

#[derive(Serialize)]
pub struct WeekScheduleResponse {
    pub sid: String,
    pub schedule: Vec<DaySchedule>,
}

#[derive(Deserialize)]
pub struct WeekScheduleRequest {
    pub schedule: Vec<DaySchedule>,
}

#[derive(Serialize)]
pub struct TodayLimitResponse {
    pub sid: String,
    pub limit_sec: i32,
    pub internet_limit_sec: i32,
    pub extra_time_min: i32,
    pub extra_internet_time_min: i32,
    pub start_time: String,
    pub end_time: String,
    pub current_screen_time: u32,
    pub current_internet_time: u32,
    pub mode: String,
    pub manual_until: String,
}

#[derive(Deserialize)]
pub struct TodayLimitRequest {
    pub limit_sec: u32,
    pub internet_limit_sec: u32,
    pub extra_time_min: i32,
    pub extra_internet_time_min: i32,
    pub start_time: String,
    pub end_time: String,
}

#[derive(Deserialize)]
struct ExtraTimeRequest {
    sid: String,
    #[serde(default)]
    screen_minutes: Option<i32>,
    #[serde(default)]
    internet_minutes: Option<i32>,
}

#[derive(Deserialize)]
struct SetExtraTimeRequest {
    sid: String,
    #[serde(default)]
    screen_minutes: Option<i32>,
    #[serde(default)]
    internet_minutes: Option<i32>,
}

#[derive(Clone)]
pub struct AppState {
    pub db: Arc<AppDb>,
    pub api_token: String,
    pub admin_cmd_tx: mpsc::Sender<AdminCommand>,
    pub env: Arc<minijinja::Environment<'static>>,
    pub config: Arc<Config>, // 🆕
}

#[derive(Serialize)]
pub struct StatusResponse {
    pub screen_time_sec: u32,
    pub daily_limit_sec: u32,
    pub is_locked: bool,
    pub session_locked: bool,
    pub lock_message: String,
    pub unlock_until: Option<String>,
    pub today_enabled: bool,
    pub current_user: String,
    pub mode: String,
    pub time_to_event: Option<TimeToEventDto>,
    pub internet_paused: bool,
    pub internet_blocked: bool, // 🆕 факт
    pub internet_time_sec: u32,
    pub internet_limit_sec: u32,
    pub device_name: String, // 🆕
}

#[derive(Deserialize)]
pub struct SetParamRequest {
    pub key: String,
    pub value: String,
}

#[derive(Serialize)]
pub struct ApiResponse {
    pub success: bool,
    pub message: String,
}

#[derive(Deserialize)]
pub struct SetPinRequest {
    pub old_pin: Option<String>,
    pub new_pin: String,
}

#[derive(Deserialize)]
pub struct ToggleControlRequest {
    pub sid: String,
    pub enabled: bool,
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TimeToEventDto {
    LockIn { minutes: u32, reason: String },
    UnlockIn { minutes: u32, reason: String },
}

impl From<TimeToEvent> for TimeToEventDto {
    fn from(ev: TimeToEvent) -> Self {
        match ev {
            TimeToEvent::LockIn { minutes, reason } => Self::LockIn { minutes, reason },
            TimeToEvent::UnlockIn { minutes, reason } => Self::UnlockIn { minutes, reason },
        }
    }
}

#[derive(Deserialize)]
pub struct ManualActionRequest {
    pub action: ManualActionKind,
    pub until: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ManualActionKind {
    Lock,
    Unlock,
    Auto,
}

// ===== Авторизация =====

fn check_auth(headers: &HeaderMap, state: &AppState) -> bool {
    check_auth_level(headers, state) != AuthLevel::None
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum AuthLevel {
    None,
    Tray,
    Admin,
}

fn check_auth_level(headers: &HeaderMap, state: &AppState) -> AuthLevel {
    let Some(auth) = headers.get("Authorization") else {
        return AuthLevel::None;
    };
    let Ok(auth_str) = auth.to_str() else {
        return AuthLevel::None;
    };

    if auth_str == format!("Bearer {}", state.api_token) {
        return AuthLevel::Admin;
    }
    if let Ok(Some(pin)) = state.db.get_setting("admin_pin") {
        if !pin.is_empty() && auth_str == format!("Bearer {}", pin) {
            return AuthLevel::Admin;
        }
    }
    if let Ok(Some(tray_token)) = state.db.get_setting("tray_token") {
        if !tray_token.is_empty() && auth_str == format!("Bearer {}", tray_token) {
            return AuthLevel::Tray;
        }
    }

    AuthLevel::None
}

// ===== Вспомогательное =====

fn device_name(state: &AppState) -> String {
    state
        .db
        .get_setting("device_name")
        .ok()
        .flatten()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| std::env::var("COMPUTERNAME").unwrap_or_else(|_| "Unknown".to_string()))
}

// ===== Обработчики =====

async fn get_status(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> AppResult<Json<StatusResponse>> {
    if !check_auth(&headers, &state) {
        return Err(AppError::Unauthorized);
    }

    let lock_message = state
        .db
        .get_setting("lock_message")?
        .unwrap_or_else(|| "🔒 КОМПЬЮТЕР ЗАБЛОКИРОВАН".to_string());

    let is_locked = state
        .db
        .get_state("lock_status")?
        .map(|s| s == "true")
        .unwrap_or(false);

    let session_locked = state
        .db
        .get_state("session_locked")?
        .map(|s| s == "true")
        .unwrap_or(false);

    let device_name = device_name(&state);

    let Some(active_sid) = get_active_user_sid() else {
        return Ok(Json(StatusResponse {
            screen_time_sec: 0,
            daily_limit_sec: 0,
            is_locked: false,
            session_locked: false,
            lock_message: "Нет активных сессий".to_string(),
            unlock_until: None,
            today_enabled: false,
            current_user: "Нет активных сессий".to_string(),
            mode: "no_user".to_string(),
            time_to_event: None,
            internet_paused: false,
            internet_blocked: false,
            internet_time_sec: 0,
            internet_limit_sec: 0,
            device_name,
        }));
    };

    let current_user = state
        .db
        .get_username_by_sid(&active_sid)
        .ok()
        .flatten()
        .unwrap_or_else(|| "Неизвестно".to_string());

    let Some(state_policy) = crate::policy::evaluate_access(&state.db, Some(&active_sid)) else {
        return Err(AppError::Internal("evaluate_access returned None".into()));
    };
    let policy = &state_policy.policy;

    let mode_str = match policy.mode {
        ControlMode::Disabled => "control_disabled",
        ControlMode::Manual {
            action: ManualAction::Lock,
            ..
        } => "manual_lock",
        ControlMode::Manual {
            action: ManualAction::Unlock,
            ..
        } => "manual_unlock",
        ControlMode::Scheduled => "scheduled",
    };

    let unlock_until = match policy.mode {
        ControlMode::Manual { until, .. } => Some(until.format("%Y-%m-%d %H:%M:%S").to_string()),
        _ => None,
    };

    let internet_paused = state.db.is_internet_paused(&active_sid).unwrap_or(false);
    let internet_blocked = state
        .db
        .get_state("internet_blocked")
        .ok()
        .flatten()
        .as_deref()
        == Some("true");
    let (_, internet_time_sec) = state.db.get_today_usage(&active_sid).unwrap_or((0, 0));
    let today_limits = state.db.ensure_today_limit(&active_sid).ok();
    let internet_limit_sec = today_limits
        .map(|t| (t.internet_limit_sec + t.extra_internet_time_min * 60).max(0) as u32)
        .unwrap_or(0);

    Ok(Json(StatusResponse {
        screen_time_sec: policy.screen_time_sec,
        daily_limit_sec: policy.limit_sec.max(0) as u32,
        is_locked,
        session_locked,
        lock_message,
        unlock_until,
        today_enabled: true,
        current_user,
        mode: mode_str.to_string(),
        time_to_event: Some(TimeToEventDto::from(policy.time_to_event.clone())),
        internet_paused,
        internet_blocked,
        internet_time_sec,
        internet_limit_sec,
        device_name,
    }))
}

async fn manual_action(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<ManualActionRequest>,
) -> AppResult<Json<ApiResponse>> {
    if !check_auth(&headers, &state) {
        return Err(AppError::Unauthorized);
    }

    let Some(sid) = get_active_user_sid() else {
        return Err(AppError::BadRequest("no active user".into()));
    };

    if let ManualActionKind::Auto = req.action {
        state.db.set_today_mode(&sid, "scheduled", "")?;

        if let Err(e) = do_sync_user_access(&state.db, &state.admin_cmd_tx, &sid).await {
            log_error!("[API manual_action] sync failed: {:?}", e);
        }

        match state.db.get_today_mode(&sid) {
            Ok((m, u)) => log_info!(
                "🔍 [API] get_today_mode вернул: mode='{}', until='{}'",
                m,
                u
            ),
            Err(e) => log_error!("❌ [API] get_today_mode error: {:?}", e),
        }

        state
            .admin_cmd_tx
            .send(AdminCommand::AutoMode)
            .await
            .map_err(|e| AppError::Internal(format!("send cmd failed: {e}")))?;
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

    let msg = if manual_until.is_empty() {
        action_str.to_string()
    } else {
        format!("{} до {}", action_str, manual_until)
    };

    Ok(Json(ApiResponse {
        success: true,
        message: msg,
    }))
}

fn resolve_manual_until(input: &str) -> Option<String> {
    if input == "-1" {
        let end_of_day = chrono::Local::now()
            .date_naive()
            .and_hms_opt(23, 59, 59)
            .unwrap();
        return Some(end_of_day.format("%Y-%m-%d %H:%M:%S").to_string());
    }
    if chrono::NaiveDateTime::parse_from_str(input, "%Y-%m-%d %H:%M:%S").is_ok() {
        return Some(input.to_string());
    }
    None
}

async fn set_param(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<SetParamRequest>,
) -> AppResult<Json<ApiResponse>> {
    if !check_auth(&headers, &state) {
        return Err(AppError::Unauthorized);
    }

    let meta = state.db.get_setting_meta(&payload.key)?;
    let Some(meta) = meta else {
        return Err(AppError::BadRequest(format!(
            "Параметр '{}' недоступен для изменения",
            payload.key
        )));
    };

    if meta.category == "internal" {
        return Err(AppError::BadRequest(format!(
            "Параметр '{}' недоступен для изменения",
            payload.key
        )));
    }

    if payload.key == "agent_mode" {
        let v = payload.value.to_lowercase();
        if v != "local" && v != "hybrid" && v != "cloud" {
            return Err(AppError::BadRequest(format!(
                "Недопустимое значение agent_mode: '{}'",
                payload.value
            )));
        }
        state.db.set_setting(&payload.key, &v)?;
    } else {
        state.db.set_setting(&payload.key, &payload.value)?;
    }

    if payload.key == "scheduler_enabled" || payload.key == "scheduler_interval_minutes" {
        let enabled = state
            .db
            .get_setting("scheduler_enabled")?
            .map(|v| v == "1" || v == "true")
            .unwrap_or(true);
        let interval = state
            .db
            .get_setting("scheduler_interval_minutes")?
            .and_then(|v| v.parse().ok())
            .unwrap_or(5);
        crate::scheduler::sync_watchdog_task(enabled, interval, &state.config.service.name);
    }

    Ok(Json(ApiResponse {
        success: true,
        message: format!("Параметр '{}' сохранён", payload.key),
    }))
}

async fn health_check(State(state): State<AppState>) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "status": "ok",
        "service": state.config.service.name,
    }))
}

async fn index_page(State(state): State<AppState>) -> AppResult<impl axum::response::IntoResponse> {
    let tmpl = state
        .env
        .get_template("index.html")
        .map_err(|e| AppError::Internal(format!("template not found: {e}")))?;

    let rendered = tmpl
        .render(context! {
            version => env!("CARGO_PKG_VERSION"),
        })
        .map_err(|e| AppError::Internal(format!("render failed: {e}")))?;

    let mut headers = axum::http::HeaderMap::new();
    headers.insert(
        header::CACHE_CONTROL,
        "no-store, no-cache, must-revalidate".parse().unwrap(),
    );
    headers.insert(header::PRAGMA, "no-cache".parse().unwrap());
    headers.insert(header::EXPIRES, "0".parse().unwrap());

    Ok((headers, Html(rendered)))
}

pub async fn get_users(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> AppResult<Json<Vec<serde_json::Value>>> {
    if !check_auth(&headers, &state) {
        return Err(AppError::Unauthorized);
    }

    let users = state.db.get_users_with_control_status()?;

    let response: Vec<serde_json::Value> = users
        .into_iter()
        .map(|(sid, username, is_enabled)| {
            serde_json::json!({
                "sid": sid,
                "username": username,
                "is_control_enabled": is_enabled
            })
        })
        .collect();

    Ok(Json(response))
}

async fn get_week_schedule(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(sid): AxumPath<String>,
) -> AppResult<Json<WeekScheduleResponse>> {
    if !check_auth(&headers, &state) {
        return Err(AppError::Unauthorized);
    }

    let schedule = state.db.get_week_schedule(&sid)?;

    Ok(Json(WeekScheduleResponse { sid, schedule }))
}

async fn save_week_schedule(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(sid): AxumPath<String>,
    Json(payload): Json<WeekScheduleRequest>,
) -> AppResult<Json<ApiResponse>> {
    if !check_auth(&headers, &state) {
        return Err(AppError::Unauthorized);
    }

    for day_schedule in payload.schedule {
        log_info!("{}:{}", day_schedule.day_of_week, day_schedule.limit_sec);
        state.db.update_day_schedule(&sid, &day_schedule)?;
    }

    if let Err(e) = do_sync_user_access(&state.db, &state.admin_cmd_tx, &sid).await {
        log_error!("[save_week_schedule] sync failed: {:?}", e);
    }

    Ok(Json(ApiResponse {
        success: true,
        message: "Расписание на неделю сохранено".to_string(),
    }))
}

async fn get_today_limit(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(sid): AxumPath<String>,
) -> AppResult<Json<TodayLimitResponse>> {
    if !check_auth(&headers, &state) {
        return Err(AppError::Unauthorized);
    }
    let limit = state.db.ensure_today_limit(&sid)?;

    let (screen_time, internet_time) = state.db.get_today_usage(&sid).unwrap_or((0, 0));

    Ok(Json(TodayLimitResponse {
        sid,
        limit_sec: limit.limit_sec,
        internet_limit_sec: limit.internet_limit_sec,
        extra_time_min: limit.extra_time_min,
        extra_internet_time_min: limit.extra_internet_time_min,
        start_time: limit.start_time,
        end_time: limit.end_time,
        current_screen_time: screen_time,
        current_internet_time: internet_time,
        mode: limit.mode,
        manual_until: limit.manual_until,
    }))
}

async fn save_today_limit(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(sid): AxumPath<String>,
    Json(payload): Json<TodayLimitRequest>,
) -> AppResult<Json<ApiResponse>> {
    if !check_auth(&headers, &state) {
        return Err(AppError::Unauthorized);
    }

    let current = state.db.ensure_today_limit(&sid)?;

    let limit = crate::types::TodayLimit {
        limit_sec: payload.limit_sec as i32,
        internet_limit_sec: payload.internet_limit_sec as i32,
        extra_time_min: payload.extra_time_min,
        extra_internet_time_min: payload.extra_internet_time_min,
        start_time: payload.start_time,
        end_time: payload.end_time,
        mode: current.mode,
        manual_until: current.manual_until,
    };

    state.db.set_today_limit(&sid, &limit)?;

    if let Err(e) = do_sync_user_access(&state.db, &state.admin_cmd_tx, &sid).await {
        log_error!("[save_today_limit] sync failed: {:?}", e);
    }

    Ok(Json(ApiResponse {
        success: true,
        message: "Лимиты на сегодня обновлены".to_string(),
    }))
}

async fn set_admin_pin(
    State(state): State<AppState>,
    _headers: HeaderMap,
    Json(payload): Json<SetPinRequest>,
) -> AppResult<Json<ApiResponse>> {
    if let Some(old_pin) = payload.old_pin {
        let stored_pin = state
            .db
            .get_setting("admin_pin")
            .ok()
            .flatten()
            .unwrap_or_default();

        if !stored_pin.is_empty() && old_pin != stored_pin {
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

async fn check_pin(
    State(state): State<AppState>,
    _headers: HeaderMap,
    Json(payload): Json<SetPinRequest>,
) -> AppResult<Json<ApiResponse>> {
    let stored_pin = state
        .db
        .get_setting("admin_pin")
        .ok()
        .flatten()
        .unwrap_or_default();

    if stored_pin.is_empty() || payload.new_pin == stored_pin {
        Ok(Json(ApiResponse {
            success: true,
            message: "✅ Авторизация успешна".to_string(),
        }))
    } else {
        Ok(Json(ApiResponse {
            success: false,
            message: "❌ Неверный PIN".to_string(),
        }))
    }
}

async fn is_pin_set(
    State(state): State<AppState>,
    _headers: HeaderMap,
) -> AppResult<Json<serde_json::Value>> {
    let pin = state
        .db
        .get_setting("admin_pin")
        .ok()
        .flatten()
        .unwrap_or_default();

    Ok(Json(serde_json::json!({
        "is_set": !pin.is_empty()
    })))
}

async fn toggle_user_control(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<ToggleControlRequest>,
) -> AppResult<Json<serde_json::Value>> {
    if !check_auth(&headers, &state) {
        return Err(AppError::Unauthorized);
    }

    state.db.set_user_control_enabled(&req.sid, req.enabled)?;

    log_info!(
        "🔄 [API] Контроль для пользователя {} {}",
        req.sid,
        if req.enabled {
            "ВКЛЮЧЕН"
        } else {
            "ВЫКЛЮЧЕН"
        }
    );

    if !req.enabled {
        if let Some(active_sid) = get_active_user_sid() {
            if active_sid == req.sid {
                log_info!("⚡ [API] Активный пользователь был заблокирован. Принудительная разблокировка...");
                let _ = state.db.set_state("lock_status", "false");
                log_info!("✅ [API] Выставлен флаг lock_status: false");
            }
        }
    }
    if let Err(e) = do_sync_user_access(&state.db, &state.admin_cmd_tx, &req.sid).await {
        log_error!("[toggle_user_control] sync failed: {:?}", e);
    }

    Ok(Json(serde_json::json!({"success": true})))
}

async fn extra_time(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<ExtraTimeRequest>,
) -> AppResult<Json<serde_json::Value>> {
    if !check_auth(&headers, &state) {
        return Err(AppError::Unauthorized);
    }

    if req.sid.is_empty() {
        return Err(AppError::BadRequest("sid is required".into()));
    }

    if req.screen_minutes.is_none() && req.internet_minutes.is_none() {
        return Err(AppError::BadRequest("nothing to update".into()));
    }

    if let Some(minutes) = req.screen_minutes {
        state.db.add_extra_time(&req.sid, minutes)?;
    }

    if let Some(minutes) = req.internet_minutes {
        state.db.add_extra_internet_time(&req.sid, minutes)?;
    }

    if let Err(e) = do_sync_user_access(&state.db, &state.admin_cmd_tx, &req.sid).await {
        log_error!("[extra_time] sync failed: {:?}", e);
    }

    Ok(Json(json!({
        "success": true,
        "sid": req.sid,
        "screen_minutes": req.screen_minutes,
        "internet_minutes": req.internet_minutes,
    })))
}

async fn set_extra_time(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<SetExtraTimeRequest>,
) -> AppResult<Json<serde_json::Value>> {
    if !check_auth(&headers, &state) {
        return Err(AppError::Unauthorized);
    }

    if req.sid.is_empty() {
        return Err(AppError::BadRequest("sid is required".into()));
    }

    if req.screen_minutes.is_none() && req.internet_minutes.is_none() {
        return Err(AppError::BadRequest("nothing to update".into()));
    }

    if let Some(minutes) = req.screen_minutes {
        state.db.set_extra_time(&req.sid, minutes)?;
    }

    if let Some(minutes) = req.internet_minutes {
        state.db.set_extra_internet_time(&req.sid, minutes)?;
    }

    if let Err(e) = do_sync_user_access(&state.db, &state.admin_cmd_tx, &req.sid).await {
        log_error!("[set_extra_time] sync failed: {:?}", e);
    }

    Ok(Json(json!({
        "success": true,
        "sid": req.sid,
        "screen_minutes": req.screen_minutes,
        "internet_minutes": req.internet_minutes,
    })))
}

async fn get_settings(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> AppResult<Json<Vec<crate::types::SettingItem>>> {
    if !check_auth(&headers, &state) {
        return Err(AppError::Unauthorized);
    }

    let mut items = state.db.get_settings()?;
    for item in items.iter_mut() {
        if item.is_secret {
            item.value = String::new();
        }
    }

    Ok(Json(items))
}

// ===== Интернет-пауза =====

async fn pause_internet(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> AppResult<Json<ApiResponse>> {
    if !check_auth(&headers, &state) {
        return Err(AppError::Unauthorized);
    }

    let Some(sid) = get_active_user_sid() else {
        return Err(AppError::BadRequest("no active user".into()));
    };

    firewall::ensure_block_rule(&sid)
        .map_err(|e| AppError::Internal(format!("firewall ensure: {e}")))?;

    firewall::set_block_enabled(&sid, true)
        .map_err(|e| AppError::Internal(format!("firewall enable: {e}")))?;

    state.db.set_internet_paused(&sid, true, "manual")?;
    state.db.set_state("internet_blocked", "true")?; // 🆕 факт

    log_info!("[API] Интернет поставлен на паузу для {}", sid);

    Ok(Json(ApiResponse {
        success: true,
        message: "🌐 Интернет на паузе".to_string(),
    }))
}

async fn resume_internet(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> AppResult<Json<ApiResponse>> {
    if !check_auth(&headers, &state) {
        return Err(AppError::Unauthorized);
    }

    let Some(sid) = get_active_user_sid() else {
        return Err(AppError::BadRequest("no active user".into()));
    };

    firewall::set_block_enabled(&sid, false)
        .map_err(|e| AppError::Internal(format!("firewall disable: {e}")))?;

    state.db.set_internet_paused(&sid, false, "")?;
    state.db.set_state("internet_blocked", "false")?; // 🆕 факт

    log_info!("[API] Интернет возобновлён для {}", sid);

    Ok(Json(ApiResponse {
        success: true,
        message: "🌐 Интернет возобновлён".to_string(),
    }))
}

async fn internet_status(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> AppResult<Json<serde_json::Value>> {
    if !check_auth(&headers, &state) {
        return Err(AppError::Unauthorized);
    }

    let Some(sid) = get_active_user_sid() else {
        return Ok(Json(serde_json::json!({
            "paused": false,
            "blocked": false,
        })));
    };

    let paused = state.db.is_internet_paused(&sid).unwrap_or(false);
    let blocked = state
        .db
        .get_state("internet_blocked")
        .ok()
        .flatten()
        .as_deref()
        == Some("true");

    Ok(Json(serde_json::json!({
        "paused": paused,
        "blocked": blocked,
    })))
}

async fn list_log_dates(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> AppResult<Json<Vec<String>>> {
    if !check_auth(&headers, &state) {
        return Err(AppError::Unauthorized);
    }

    let logs_dir = std::path::Path::new(&state.config.paths.log_dir).join("logs");
    let mut dates: Vec<String> = std::fs::read_dir(&logs_dir)
        .map_err(|e| AppError::Internal(format!("read logs dir: {e}")))?
        .filter_map(|e| e.ok())
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|name| chrono::NaiveDate::parse_from_str(name, "%Y-%m-%d").is_ok())
        .collect();

    dates.sort();
    dates.reverse();
    Ok(Json(dates))
}

async fn get_logs(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(date): AxumPath<String>,
    Query(params): Query<LogQuery>,
) -> AppResult<Json<serde_json::Value>> {
    if !check_auth(&headers, &state) {
        return Err(AppError::Unauthorized);
    }

    // Валидация date
    if chrono::NaiveDate::parse_from_str(&date, "%Y-%m-%d").is_err() {
        return Err(AppError::BadRequest("invalid date".into()));
    }

    let logs_dir = std::path::Path::new(&state.config.paths.log_dir)
        .join("logs")
        .join(&date);

    if !logs_dir.exists() {
        return Ok(Json(json!({ "lines": [], "files": [] })));
    }

    let source = params.source.as_deref().unwrap_or("service");
    let min_level = params.level.as_deref().unwrap_or("info");
    let limit = params.limit.unwrap_or(500).min(5000);

    let file_path = logs_dir.join(format!("{}.log", source));
    let content = std::fs::read_to_string(&file_path).unwrap_or_default();

    let lines: Vec<&str> = content
        .lines()
        .filter(|line| match min_level {
            "error" => line.contains("[ERROR]"),
            "warn" => line.contains("[WARN ]") || line.contains("[ERROR]"),
            "info" => {
                line.contains("[INFO ]") || line.contains("[WARN ]") || line.contains("[ERROR]")
            }
            "debug" => true,
            _ => true,
        })
        .rev()
        .take(limit)
        .collect();

    Ok(Json(json!({
        "date": date,
        "source": source,
        "level": min_level,
        "count": lines.len(),
        "lines": lines,
    })))
}

#[derive(Deserialize)]
struct LogQuery {
    source: Option<String>,
    level: Option<String>,
    limit: Option<usize>,
}
// ===== Логирование =====
/*
fn debug_log(log_dir: &str, msg: &str) {
    println!("{}", msg);

    let log_path = format!("{}\\api_debug.log", log_dir);
    let _ = std::fs::create_dir_all(log_dir);

    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(&log_path) {
        let timestamp = chrono::Local::now().format("[%Y-%m-%d %H:%M:%S]");
        let _ = writeln!(file, "{} {}", timestamp, msg);
    }
}
*/
// ===== Запуск =====

pub async fn start_api_server(
    db: Arc<AppDb>,
    config: Arc<Config>,
    admin_cmd_tx: mpsc::Sender<AdminCommand>,
) {
    log_debug!("🔌 [API] Функция start_api_server НАЧАЛА выполнение");

    let api_token = match db.get_state("api_token") {
        Ok(Some(token)) if !token.is_empty() => token,
        Ok(_) => {
            let token = format!(
                "agent_{}",
                uuid::Uuid::new_v4()
                    .simple()
                    .to_string()
                    .chars()
                    .take(8)
                    .collect::<String>()
            );
            if let Err(e) = db.set_setting("api_token", &token) {
                log_error!("❌ [API] Не удалось записать токен: {}", e);
                return;
            }
            log_info!("🔑 [API] Сгенерирован новый токен: {}", token);
            token
        }
        Err(e) => {
            log_error!("❌ [API] Ошибка чтения токена: {}", e);
            return;
        }
    };

    let tray_token = match db.get_setting("tray_token") {
        Ok(Some(t)) if !t.is_empty() => t,
        _ => {
            let token = format!("tray_{}", uuid::Uuid::new_v4().simple());
            if let Err(e) = db.set_setting("tray_token", &token) {
                log_error!("[device] не удалось записать tray_token: {:?}", e);
                return;
            }
            log_info!("🔑 [device] сгенерирован tray_token");
            token
        }
    };

    // tray_token.txt — в рабочей папке из config
    let tray_token_path = Path::new(&config.paths.log_dir).join("tray_token.txt");
    if let Err(e) = std::fs::write(&tray_token_path, &tray_token) {
        log_error!("[device] не удалось записать tray_token.txt: {:?}", e);
    }

    let exe_path = match env::current_exe() {
        Ok(p) => p,
        Err(e) => {
            log_error!("❌ [API] Не удалось получить путь к exe: {}", e);
            return;
        }
    };
    let exe_dir = exe_path.parent().unwrap();

    let templates_dir = exe_dir.join("templates");

    if templates_dir.exists() {
        log_debug!("✅ [API] Найдена папка templates: {:?}", templates_dir);
    } else {
        log_warn!("⚠️ [API] Папка templates НЕ НАЙДЕНА: {:?}", templates_dir);
    }

    let mut env_builder = minijinja::Environment::new();
    env_builder.set_loader(minijinja::path_loader(templates_dir.clone()));

    let env = Arc::new(env_builder);

    let state = AppState {
        db,
        api_token,
        admin_cmd_tx,
        env,
        config: Arc::clone(&config),
    };

    log_debug!("✅ [API] AppState создан");

    let final_static_dir = exe_dir.join(&config.paths.static_dir);

    if final_static_dir.exists() {
        log_debug!("✅ [API] Найдена папка static: {:?}", final_static_dir);
    } else {
        log_warn!("⚠️ [API] Папка static НЕ НАЙДЕНА: {:?}", final_static_dir);
    }

    log_debug!("🔧 [API] Попытка сборки Router...");

    let build_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        Router::new()
            .route("/", get(index_page))
            .route("/health", get(health_check))
            .route("/api/status", get(get_status))
            .route("/api/set_param", post(set_param))
            .route("/api/users", get(get_users))
            .route(
                "/api/schedule/{sid}",
                get(get_week_schedule).post(save_week_schedule),
            )
            .route(
                "/api/today/{sid}",
                get(get_today_limit).post(save_today_limit),
            )
            .route("/api/manual_action", post(manual_action))
            .route("/api/extra_time", post(extra_time))
            .route("/api/set_extra_time", post(set_extra_time))
            .route("/api/settings", get(get_settings))
            .route("/api/check_pin", post(check_pin))
            .route("/api/set_pin", post(set_admin_pin))
            .route("/api/is_pin_set", get(is_pin_set))
            .route("/api/users/toggle_control", post(toggle_user_control))
            .route("/api/internet/pause", post(pause_internet))
            .route("/api/internet/resume", post(resume_internet))
            .route("/api/internet/status", get(internet_status))
            .route("/api/logs", get(list_log_dates))
            .route("/api/logs/{date}", get(get_logs))
            .route(
                "/switch-server",
                get(|| async { axum::response::Redirect::to("/") }),
            )
            .fallback_service(
                ServeDir::new(final_static_dir).append_index_html_on_directories(true),
            )
            .with_state(state)
    }));

    match build_result {
        Ok(app) => {
            log_debug!("✅ [API] Router ПОЛНОСТЬЮ собран и готов к запуску!");

            let addr = format!("0.0.0.0:{}", config.service.api_port);
            log_info!("📡 [API] Биндимся к адресу: {}", addr);

            match tokio::net::TcpListener::bind(&addr).await {
                Ok(listener) => {
                    log_info!("🚀 [API] Сервер УСПЕШНО запущен на http://{}", addr);
                    if let Err(e) = axum::serve(listener, app).await {
                        log_error!("❌ [API] Сервер упал во время работы: {}", e);
                    }
                }
                Err(e) => {
                    log_error!("❌ [API] НЕ УДАЛОСЬ занять порт {}. Причина: {}", addr, e);
                }
            }
        }
        Err(panic_info) => {
            let msg = if let Some(s) = panic_info.downcast_ref::<&str>() {
                format!("💥 [API] КРИТИЧЕСКАЯ ПАНИКА при сборке Router: {}", s)
            } else if let Some(s) = panic_info.downcast_ref::<String>() {
                format!("💥 [API] КРИТИЧЕСКАЯ ПАНИКА при сборке Router: {}", s)
            } else {
                "💥 [API] КРИТИЧЕСКАЯ ПАНИКА при сборке Router: Неизвестная причина".to_string()
            };
            log_error!("{}", msg);
            log_error!("💡 Подсказка: Проверьте, нет ли дублирующихся .route() с одинаковыми путями в api.rs");
            return;
        }
    }
}
