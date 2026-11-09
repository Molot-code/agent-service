use serde::{Deserialize, Serialize};

use crate::types::{ControlMode, DaySchedule, ManualAction, TimeToEvent};

// ---------- response ----------

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
    /// Причина ручного режима: "" (ручной из UI) или "not_accepted"
    /// (ежедневное подтверждение не получено).
    pub mode_reason: String,
    pub time_to_event: Option<TimeToEventDto>,
    pub internet_paused: bool,
    pub internet_blocked: bool,
    pub internet_time_sec: u32,
    pub internet_limit_sec: u32,
    pub device_name: String,
}

#[derive(Serialize)]
pub struct ApiResponse {
    pub success: bool,
    pub message: String,
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

// ---------- request ----------

#[derive(Deserialize)]
pub struct WeekScheduleRequest {
    pub schedule: Vec<DaySchedule>,
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
pub struct SetParamRequest {
    pub key: String,
    pub value: String,
}

#[derive(Deserialize)]
pub struct SetPinRequest {
    pub old_pin: Option<String>,
    pub new_pin: String,
}

#[derive(Deserialize)]
pub struct CheckPinRequest {
    pub pin: String,
}

#[derive(Deserialize)]
pub struct ToggleControlRequest {
    pub sid: String,
    pub enabled: bool,
}

#[derive(Deserialize)]
pub struct ReturnToScheduledRequest {
    pub sid: String,
}

#[derive(Deserialize)]
pub struct ExtraTimeRequest {
    pub sid: String,
    #[serde(default)]
    pub screen_minutes: Option<i32>,
    #[serde(default)]
    pub internet_minutes: Option<i32>,
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

#[derive(Deserialize)]
pub struct LogQuery {
    pub source: Option<String>,
    pub level: Option<String>,
    pub limit: Option<usize>,
}

// ---------- helpers: ControlMode → строка ----------

impl ControlMode {
    pub fn as_str(&self) -> &'static str {
        match self {
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
        }
    }

    /// Причина ручного режима ("" — ручной из UI, "not_accepted" —
    /// ежедневное подтверждение не получено). Для неручных режимов — "".
    pub fn reason(&self) -> &str {
        match self {
            ControlMode::Manual { reason, .. } => reason.as_str(),
            _ => "",
        }
    }
}

#[derive(Serialize)]
pub struct DefaultsResponse {
    /// Один день расписания: используется и для `today`, и для каждого дня `weekDays`.
    pub day: DayDefaults,
    /// Порядок дней недели для фронта: `["mon", "tue", ..., "sun"]`.
    /// `day_of_week` для каждого дня — 1..6, 0.
    pub week: Vec<WeekDayInfo>,
}

#[derive(Serialize)]
pub struct DayDefaults {
    pub start_time: String,      // "08:00"
    pub end_time: String,        // "20:00"
    pub limit_min: i32,          // 120
    pub internet_limit_min: i32, // 120
}

#[derive(Serialize)]
pub struct WeekDayInfo {
    pub day_of_week: u32, // 0=Sun..6=Sat (как в БД)
    pub name: String,     // "Понедельник"
    /// Индекс для фронтового массива `weekDays` (Пн=0..Вс=6).
    pub frontend_idx: usize,
}

#[derive(Serialize)]
pub struct ActivityLogResponse {
    pub entries: Vec<ActivityLogEntryDto>,
}

#[derive(Serialize)]
pub struct ActivityLogEntryDto {
    pub id: i64,
    pub event_type: String,
    pub details: String,
    pub screen_time_sec: i32,
    pub internet_time_sec: i32,
    pub screen_limit_sec: i32,
    pub internet_limit_sec: i32,
    pub is_locked: bool,
    pub internet_paused: bool,
    pub created_at: String,
    pub state: String, // 🆕 Добавили
}
