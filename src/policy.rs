// src/policy.rs
use std::sync::Arc;

use chrono::{DateTime, Datelike, Local, NaiveDateTime, NaiveTime, Timelike};
use tokio::sync::mpsc;

use crate::{
    db::AppDb,
    log_debug, log_info,
    types::{AdminCommand, ControlMode, DaySchedule, ManualAction, TimeToEvent, TodayLimit},
    users::get_active_user_sid,
};

/// Верхняя граница для time_to_event — 7 дней в минутах.
const WEEK_MINUTES: u32 = 7 * 24 * 60;

// ==========================================
// ПУБЛИЧНЫЕ ТИПЫ
// ==========================================

#[derive(Debug, Clone)]
pub struct PolicyCheckResult {
    pub mode: ControlMode,
    pub is_exceeded: bool,
    pub screen_time_sec: u32,
    pub limit_sec: i32,
    pub in_schedule: bool,
    pub time_to_event: TimeToEvent,
    pub internet_time_sec: u32,
    pub internet_limit_sec: i32,
    pub internet_exceeded: bool,
}

#[derive(Debug, Clone)]
pub struct AccessState {
    pub policy: PolicyCheckResult,
}

// ==========================================
// ГЛАВНЫЕ ФУНКЦИИ
// ==========================================

/// Главная точка входа для Service.rs.
pub fn evaluate_access(db: &AppDb, sid_opt: Option<&str>) -> Option<AccessState> {
    let sid = resolve_sid(sid_opt)?;
    let week = db.get_week_schedule(&sid).unwrap_or_default();
    let is_locked = read_lock_status(db);
    let policy = check_screen_time_policy(db, Some(&sid), &week, is_locked)?;
    Some(AccessState { policy })
}

pub fn check_screen_time_policy(
    db: &AppDb,
    sid_opt: Option<&str>,
    week_schedule: &[DaySchedule],
    is_locked: bool,
) -> Option<PolicyCheckResult> {
    let sid = resolve_sid(sid_opt)?;

    // 1. Контроль выключен
    if !db.is_user_control_enabled(&sid).unwrap_or(true) {
        return Some(PolicyCheckResult {
            mode: ControlMode::Disabled,
            is_exceeded: false,
            screen_time_sec: 0,
            limit_sec: 0,
            in_schedule: true,
            time_to_event: TimeToEvent::LockIn {
                minutes: 0,
                reason: String::new(),
            },
            internet_time_sec: 0,
            internet_limit_sec: 0,
            internet_exceeded: false,
        });
    }

    let today_limits = match db.ensure_today_limit(&sid) {
        Ok(l) => l,
        Err(_) => return None, // БД недоступна — политика неизвестна
    };

    let (screen_time, internet_time) = db.get_today_usage(&sid).unwrap_or((0, 0));

    let effective_limit_sec = today_limits.limit_sec + today_limits.extra_time_min * 60;
    let effective_internet_limit_sec =
        today_limits.internet_limit_sec + today_limits.extra_internet_time_min * 60;

    let internet_exceeded = (internet_time as i32) >= effective_internet_limit_sec.max(0);
    let now_time = Local::now().format("%H:%M").to_string();
    let in_schedule = !(now_time < today_limits.start_time || now_time > today_limits.end_time);
    let is_exceeded = (screen_time as i32) >= effective_limit_sec.max(0);

    log_debug!("is_exceeded: {}", is_exceeded);

    // 2. Определяем режим
    let mode = parse_control_mode(&today_limits);

    let mut result = PolicyCheckResult {
        mode,
        is_exceeded,
        screen_time_sec: screen_time,
        limit_sec: effective_limit_sec,
        in_schedule,
        time_to_event: TimeToEvent::LockIn {
            minutes: 0,
            reason: String::new(),
        },
        internet_time_sec: internet_time,
        internet_limit_sec: effective_internet_limit_sec,
        internet_exceeded,
    };

    result.time_to_event = calculate_time_to_event(
        &today_limits,
        week_schedule,
        &result,
        is_locked,
        Local::now(),
    );

    Some(result)
}

/// Обёртка над `evaluate_access`: «есть ли доступ прямо сейчас».
pub fn is_access_allowed(db: &AppDb, sid_opt: Option<&str>) -> bool {
    match evaluate_access(db, sid_opt) {
        None => true,
        Some(state) => !should_lock(&state.policy),
    }
}

/// Решает: система ДОЛЖНА быть заблокирована по политике?
pub fn should_lock(policy: &PolicyCheckResult) -> bool {
    match policy.mode {
        ControlMode::Disabled => false,
        ControlMode::Manual {
            action: ManualAction::Lock,
            ..
        } => true,
        ControlMode::Manual {
            action: ManualAction::Unlock,
            ..
        } => false,
        ControlMode::Scheduled => !policy.in_schedule || policy.is_exceeded,
    }
}

pub async fn do_sync_user_access(
    db: &Arc<AppDb>,
    admin_cmd_tx: &mpsc::Sender<AdminCommand>,
    sid: &str,
) -> Result<(), ()> {
    if sid.is_empty() || get_active_user_sid().as_deref() != Some(sid) {
        return Ok(());
    }

    let Some(state) = evaluate_access(db, Some(sid)) else {
        return Ok(());
    };

    if should_lock(&state.policy) {
        log_info!("[sync] → блокировка для {}", sid);
        let _ = db.set_state("lock_status", "true");
        let _ = admin_cmd_tx.send(AdminCommand::ForceLock).await;
    } else {
        log_info!("[sync] → разблокировка для {}", sid);
        let _ = admin_cmd_tx.send(AdminCommand::ForceUnlock).await;
    }
    Ok(())
}

// ==========================================
// ЛОГИКА ВРЕМЕНИ
// ==========================================

fn calculate_time_to_event(
    today_limits: &TodayLimit,
    week_schedule: &[DaySchedule],
    policy: &PolicyCheckResult,
    is_locked: bool,
    now: DateTime<Local>,
) -> TimeToEvent {
    match policy.mode {
        ControlMode::Disabled => TimeToEvent::LockIn {
            minutes: 0,
            reason: String::new(),
        },

        ControlMode::Manual {
            until,
            action,
            ref reason,
        } => {
            let minutes = (until - now.naive_local()).num_minutes().max(0) as u32;
            let r = if reason == "not_accepted" {
                "not_accepted"
            } else {
                "manual"
            };
            match action {
                ManualAction::Lock => TimeToEvent::UnlockIn {
                    minutes: minutes.min(WEEK_MINUTES),
                    reason: r.into(),
                },
                ManualAction::Unlock => TimeToEvent::LockIn {
                    minutes: minutes.min(WEEK_MINUTES),
                    reason: r.into(),
                },
            }
        }

        ControlMode::Scheduled => {
            if is_locked {
                let m = find_next_schedule_start(week_schedule, now);
                TimeToEvent::UnlockIn {
                    minutes: m.min(WEEK_MINUTES),
                    reason: "schedule".into(),
                }
            } else {
                let minutes_to_limit = if policy.limit_sec > 0 {
                    let remaining =
                        (policy.limit_sec - policy.screen_time_sec as i32).max(0) as u32;
                    Some(remaining / 60)
                } else {
                    None
                };
                let minutes_to_end = minutes_until_end_time(&today_limits.end_time, now);

                let (m, reason) = match (minutes_to_limit, minutes_to_end) {
                    (Some(a), Some(b)) if a <= b => (a, "limit"),
                    (Some(_), Some(b)) => (b, "schedule"),
                    (Some(a), None) => (a, "limit"),
                    (None, Some(b)) => (b, "schedule"),
                    (None, None) => (0, ""),
                };

                TimeToEvent::LockIn {
                    minutes: m.min(WEEK_MINUTES),
                    reason: reason.into(),
                }
            }
        }
    }
}

// ==========================================
// ХЕЛПЕРЫ
// ==========================================

/// Возвращает sid: переданный или активного пользователя.
fn resolve_sid(sid_opt: Option<&str>) -> Option<String> {
    match sid_opt {
        Some(s) if !s.is_empty() => Some(s.to_string()),
        _ => get_active_user_sid(),
    }
}

/// Читает `lock_status` из глобальных настроек.
fn read_lock_status(db: &AppDb) -> bool {
    db.get_state("lock_status").ok().flatten().as_deref() == Some("true")
}

/// Определяет `ControlMode` на основе полей `daily_limits`.
/// Если manual_until пуст или истёк — возвращает Scheduled.
fn parse_control_mode(today_limits: &TodayLimit) -> ControlMode {
    let until = match NaiveDateTime::parse_from_str(&today_limits.manual_until, "%Y-%m-%d %H:%M:%S")
    {
        Ok(dt) if Local::now().naive_local() < dt => dt,
        _ => return ControlMode::Scheduled,
    };

    match today_limits.mode.as_str() {
        "manual_lock" => ControlMode::Manual {
            until,
            action: ManualAction::Lock,
            reason: String::new(),
        },
        "manual_unlock" => ControlMode::Manual {
            until,
            action: ManualAction::Unlock,
            reason: String::new(),
        },
        "not_accepted" => ControlMode::Manual {
            until,
            action: ManualAction::Lock,
            reason: "not_accepted".into(),
        },
        _ => ControlMode::Scheduled,
    }
}

/// "08:00" → 480 минут.
fn parse_time_to_minutes(s: &str) -> Option<u32> {
    NaiveTime::parse_from_str(s, "%H:%M")
        .ok()
        .map(|t| t.hour() * 60 + t.minute())
}

/// Преобразует chrono::Weekday в формат БД (Вс=0, Пн=1, ..., Сб=6).
fn chrono_weekday_to_db(weekday: chrono::Weekday) -> u32 {
    (weekday.num_days_from_monday() + 1) % 7
}

/// Минут до `end_time` **сегодня**. `None`, если окно уже закрыто.
fn minutes_until_end_time(end_time: &str, now: DateTime<Local>) -> Option<u32> {
    let end = parse_time_to_minutes(end_time)?;
    let end_dt = now.date_naive().and_hms_opt(end / 60, end % 60, 0)?;
    if end_dt > now.naive_local() {
        Some((end_dt - now.naive_local()).num_minutes().max(0) as u32)
    } else {
        None
    }
}

/// Ищет ближайший `start_time` в будущем.
fn find_next_schedule_start(week: &[DaySchedule], now: DateTime<Local>) -> u32 {
    let now_naive = now.naive_local();
    let today_dow_db = chrono_weekday_to_db(now.date_naive().weekday());

    for offset in 0..7u32 {
        let target_dow = (today_dow_db + offset) % 7;
        let Some(day) = week.iter().find(|d| d.day_of_week == target_dow as i32) else {
            continue;
        };
        let Some(start) = parse_time_to_minutes(&day.start_time) else {
            continue;
        };

        let target_date = now.date_naive() + chrono::Duration::days(offset as i64);
        let Some(start_dt) = target_date.and_hms_opt(start / 60, start % 60, 0) else {
            continue;
        };

        if start_dt > now_naive {
            return (start_dt - now_naive).num_minutes().max(0) as u32;
        }
    }
    WEEK_MINUTES
}
