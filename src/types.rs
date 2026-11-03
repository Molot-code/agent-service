// src/types.rs
use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq)]
pub enum SessionState {
    Active,
    Locked,    // Win+L или скринсейвер
    Logoff,    // Пользователь вышел
    Suspended, // Сон / Гибернация
}

// types.rs
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TimeToEvent {
    LockIn {
        minutes: u32,
        reason: String, // 🆕 "limit" | "schedule" | "manual" | ""
    },
    UnlockIn {
        minutes: u32,
        reason: String, // 🆕 "manual" | "schedule" | ""
    },
}

#[derive(Debug, Clone)]
pub enum AdminCommand {
    ForceUnlock, // Ручная разблокировка (переход в ручной режим)
    ForceLock,   // Ручная блокировка (переход в ручной режим, экран заблокирован)
    AutoMode,    //  Возврат в режим "По расписанию" (сброс ручных флагов)
    SyncUsers,   // Принудительная синхронизация пользователей
}

// src/types.rs
/// Режим контроля для пользователя на текущий момент.
/// ЕДИНСТВЕННЫЙ источник правды для Service.rs о том,
/// как обрабатывать блокировку и счётчик.
#[derive(Debug, Clone, PartialEq)]
pub enum ControlMode {
    /// Контроль выключен для пользователя.
    /// Service.rs: снять блокировку, счётчик не считать.
    Disabled,

    /// Ручной режим активен до `until`, с указанием цели — держать блок или разблок.
    /// Service.rs:
    ///   - ManualLock: держать блокировку (если снята — ставить снова).
    ///   - ManualUnlock: держать разблокировку (если стоит — снимать).
    /// Расписание и лимиты игнорируются.
    /// `reason` — причина: "" (ручной режим из UI) или "not_accepted"
    /// (ежедневное подтверждение не получено).
    Manual {
        until: NaiveDateTime,
        action: ManualAction,
        reason: String,
    },

    /// Обычная логика: лимит + расписание.
    /// Service.rs: решать по `in_schedule` и `is_exceeded`.
    Scheduled,
}

/// Цель ручного режима.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ManualAction {
    Lock,
    Unlock,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TodayLimit {
    pub limit_sec: i32,
    pub internet_limit_sec: i32,
    pub extra_time_min: i32,
    pub extra_internet_time_min: i32,
    pub start_time: String,
    pub end_time: String,
    pub mode: String,         // "scheduled" | "manual"
    pub manual_until: String, // "" | "YYYY-MM-DD HH:MM:SS"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaySchedule {
    pub day_of_week: i32,
    pub limit_sec: i32,
    pub internet_limit_sec: i32,
    pub start_time: String,
    pub end_time: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SettingItem {
    pub key: String,
    pub value: String,
    pub category: String,
    pub value_type: String,
    pub label: String,
    pub description: String,
    pub sort_order: i32,
    pub is_secret: bool,
}
