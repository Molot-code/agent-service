use std::path::Path;

use axum::{
    extract::{Path as AxumPath, Query, State},
    http::header,
    response::Html,
    Json,
};
use minijinja::context;
use serde::Deserialize;
use serde_json::json;

use super::super::{
    dto::{
        ActivityLogResponse, LogQuery, StatusResponse, TimeToEventDto, TodayLimitRequest,
        TodayLimitResponse, WeekScheduleRequest, WeekScheduleResponse,
    },
    util::device_name,
    AppState,
};
use crate::{
    error::{AppError, AppResult},
    log_error, log_info,
    policy::{do_sync_user_access, evaluate_access},
    types::SettingItem,
    users::get_active_user_sid,
};

pub async fn index_page(
    State(state): State<AppState>,
) -> AppResult<impl axum::response::IntoResponse> {
    let tmpl = state
        .env
        .get_template("index.html")
        .map_err(|e| AppError::Internal(format!("template not found: {e}")))?;

    let rendered = tmpl
        .render(context! { version => env!("CARGO_PKG_VERSION") })
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

pub async fn get_status(State(state): State<AppState>) -> AppResult<Json<StatusResponse>> {
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

    let Some(ev) = evaluate_access(&state.db, Some(&active_sid)) else {
        return Err(AppError::Internal("evaluate_access returned None".into()));
    };
    let policy = &ev.policy;

    let unlock_until = match policy.mode {
        crate::types::ControlMode::Manual { until, .. } => {
            Some(until.format("%Y-%m-%d %H:MM:SS").to_string())
        }
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
    let internet_limit_sec = state
        .db
        .ensure_today_limit(&active_sid)
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
        mode: policy.mode.as_str().to_string(),
        time_to_event: Some(TimeToEventDto::from(policy.time_to_event.clone())),
        internet_paused,
        internet_blocked,
        internet_time_sec,
        internet_limit_sec,
        device_name,
    }))
}

pub async fn get_users(State(state): State<AppState>) -> AppResult<Json<Vec<serde_json::Value>>> {
    let users = state.db.get_users_with_control_status()?;
    let response: Vec<_> = users
        .into_iter()
        .map(|(sid, username, is_enabled)| {
            json!({ "sid": sid, "username": username, "is_control_enabled": is_enabled })
        })
        .collect();
    Ok(Json(response))
}

pub async fn get_week_schedule(
    State(state): State<AppState>,
    AxumPath(sid): AxumPath<String>,
) -> AppResult<Json<WeekScheduleResponse>> {
    let schedule = state.db.get_week_schedule(&sid)?;
    Ok(Json(WeekScheduleResponse { sid, schedule }))
}

pub async fn save_week_schedule(
    State(state): State<AppState>,
    AxumPath(sid): AxumPath<String>,
    Json(payload): Json<WeekScheduleRequest>,
) -> AppResult<Json<crate::api::ApiResponse>> {
    for day_schedule in payload.schedule {
        log_info!("{}:{}", day_schedule.day_of_week, day_schedule.limit_sec);
        state.db.update_day_schedule(&sid, &day_schedule)?;
    }
    if let Err(e) = do_sync_user_access(&state.db, &state.admin_cmd_tx, &sid).await {
        log_error!("[save_week_schedule] sync failed: {:?}", e);
    }
    Ok(Json(crate::api::ApiResponse {
        success: true,
        message: "Расписание на неделю сохранено".to_string(),
    }))
}

pub async fn get_today_limit(
    State(state): State<AppState>,
    AxumPath(sid): AxumPath<String>,
) -> AppResult<Json<TodayLimitResponse>> {
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

pub async fn save_today_limit(
    State(state): State<AppState>,
    AxumPath(sid): AxumPath<String>,
    Json(payload): Json<TodayLimitRequest>,
) -> AppResult<Json<crate::api::ApiResponse>> {
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
    Ok(Json(crate::api::ApiResponse {
        success: true,
        message: "Лимиты на сегодня обновлены".to_string(),
    }))
}

pub async fn get_settings(State(state): State<AppState>) -> AppResult<Json<Vec<SettingItem>>> {
    let mut items = state.db.get_settings()?;
    for item in items.iter_mut() {
        if item.is_secret {
            item.value = String::new();
        }
    }
    Ok(Json(items))
}

pub async fn list_log_dates(State(state): State<AppState>) -> AppResult<Json<Vec<String>>> {
    let dir = Path::new(&state.config.paths.log_dir);
    log_info!(
        "🔍 [API] Ищу логи в папке: {:?} (существует: {})",
        dir,
        dir.exists()
    );
    let mut dates: Vec<String> = std::fs::read_dir(&dir)
        .map_err(|e| AppError::Internal(format!("read logs dir: {e}")))?
        .filter_map(|e| e.ok())
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|name| chrono::NaiveDate::parse_from_str(name, "%Y-%m-%d").is_ok())
        .collect();
    dates.sort();
    dates.reverse();
    Ok(Json(dates))
}

pub async fn get_logs(
    State(state): State<AppState>,
    AxumPath(date): AxumPath<String>,
    Query(params): Query<LogQuery>,
) -> AppResult<Json<serde_json::Value>> {
    if chrono::NaiveDate::parse_from_str(&date, "%Y-%m-%d").is_err() {
        return Err(AppError::BadRequest("invalid date".into()));
    }

    let dir = Path::new(&state.config.paths.log_dir).join(&date);
    if !dir.exists() {
        return Ok(Json(json!({ "lines": [], "files": [] })));
    }

    let source = params.source.as_deref().unwrap_or("service");
    let min_level = params.level.as_deref().unwrap_or("info");
    let limit = params.limit.unwrap_or(500).min(5000);

    let content = std::fs::read_to_string(dir.join(format!("{source}.log"))).unwrap_or_default();
    let lines: Vec<&str> = content
        .lines()
        .filter(|line| level_matches(line, min_level))
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

fn level_matches(line: &str, min_level: &str) -> bool {
    match min_level {
        "error" => line.contains("[ERROR]"),
        "warn" => line.contains("[WARN ]") || line.contains("[ERROR]"),
        "info" => line.contains("[INFO ]") || line.contains("[WARN ]") || line.contains("[ERROR]"),
        _ => true,
    }
}

use super::super::dto::{ApiResponse, SetParamRequest};
use crate::scheduler;

pub async fn set_param(
    State(state): State<AppState>,
    Json(payload): Json<SetParamRequest>,
) -> AppResult<Json<ApiResponse>> {
    let Some(meta) = state.db.get_setting_meta(&payload.key)? else {
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
        scheduler::sync_watchdog_task(enabled, interval, &state.config.service.name);
    }

    Ok(Json(ApiResponse {
        success: true,
        message: format!("Параметр '{}' сохранён", payload.key),
    }))
}

#[derive(Debug, Deserialize)]
pub struct ActivityLogQuery {
    /// YYYY-MM-DD; если не задан — берём сегодня
    pub date: Option<String>,
}

pub async fn get_activity_log(
    State(state): State<AppState>,
    AxumPath(sid): AxumPath<String>,
    Query(q): Query<ActivityLogQuery>,
) -> AppResult<Json<ActivityLogResponse>> {
    let entries = state.db.get_activity_log(&sid, q.date.as_deref(), 1000)?;

    let dtos: Vec<super::super::dto::ActivityLogEntryDto> = entries
        .into_iter()
        .map(|e| super::super::dto::ActivityLogEntryDto {
            id: e.id,
            event_type: e.event_type,
            details: e.details,
            screen_time_sec: e.screen_time_sec,
            internet_time_sec: e.internet_time_sec,
            screen_limit_sec: e.screen_limit_sec,
            internet_limit_sec: e.internet_limit_sec,
            is_locked: e.is_locked != 0,
            internet_paused: e.internet_paused != 0,
            created_at: e.created_at,
            state: e.state,
        })
        .collect();

    Ok(Json(ActivityLogResponse { entries: dtos }))
}
