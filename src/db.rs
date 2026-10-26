// src/db.rs
use std::{
    path::{Path, PathBuf},
    sync::Mutex,
};

use chrono::Datelike;
use rusqlite::{params, Connection, Result};

use crate::{
    config::Config,
    log_info,
    types::{DaySchedule, SettingItem, TodayLimit},
};

pub const DEFAULT_DB_DIR: &str = r"C:\ProgramData\ParentalControl";

pub struct AppDb {
    conn: Mutex<Connection>,
}

pub struct SettingMeta {
    pub value_type: String,
    pub label: String,
    pub description: String,
    pub category: String,
}

impl AppDb {
    /// Создаёт БД в указанной директории.
    /// Директория создаётся, если её нет.
    pub fn new<P: AsRef<Path>>(db_dir: P) -> Result<Self> {
        let db_dir = db_dir.as_ref();
        std::fs::create_dir_all(db_dir).ok();

        let db_path = db_dir.join("data.db");
        let conn = Connection::open(db_path)?;

        conn.execute_batch("PRAGMA journal_mode=WAL;")?;
        conn.execute_batch("PRAGMA foreign_keys=ON;")?;

        conn.execute_batch(
            "
            -- Метаданные устройства
            CREATE TABLE IF NOT EXISTS device_info (
                device_id TEXT PRIMARY KEY,
                device_name TEXT NOT NULL,
                created_at TEXT DEFAULT (datetime('now'))
            );

            -- Внутренние состояния системы (флаги, статусы). Редактируется только кодом.
            CREATE TABLE IF NOT EXISTS system_state (
                key        TEXT PRIMARY KEY,
                value      TEXT NOT NULL,
                updated_at TEXT NOT NULL DEFAULT (datetime('now'))
            );

            -- Пользовательские настройки (редактирует родитель через UI).
            CREATE TABLE IF NOT EXISTS settings (
                key              TEXT PRIMARY KEY,
                value            TEXT NOT NULL,
                category         TEXT NOT NULL DEFAULT 'general',
                value_type       TEXT NOT NULL DEFAULT 'string',
                label            TEXT NOT NULL DEFAULT '',
                description      TEXT NOT NULL DEFAULT '',
                sort_order       INTEGER NOT NULL DEFAULT 0,
                is_secret        INTEGER NOT NULL DEFAULT 0,
                is_user_editable INTEGER NOT NULL DEFAULT 1
            );

            -- Дефолтные настройки
        INSERT OR IGNORE INTO settings
            (key, value, category, value_type, label, description, sort_order, is_secret)
        VALUES
            ('lock_message',    '🔒 Компьютер заблокирован', 'notifications', 'multiline', 'Сообщение на экране блокировки', '', 10, 0),
            ('lock_delay_sec',  '5',                         'timing',        'int',       'Задержка блокировки (сек)',       'Сколько секунд показывать окно перед блокировкой', 10, 0),
            ('admin_pin',       '',                           'security',      'password',  'PIN-код родителя',                '', 10, 1),
            ('device_name',     '',                           'general',       'string',    'Имя устройства',                  'Как это устройство видят другие в сети', 10, 0),
            ('api_token',  '', 'internal', 'string', 'API токен', 'API токен', 10, 0),
            ('tray_token', '', 'internal', 'string', 'Tray токен', 'Tray токен', 10, 0),
            ('lock_internet_on_screen_lock', '0', 'control', 'bool', 'Блокировать интернет при блокировке экрана', 'Если включено, при блокировке экрана интернет будет отключаться', 20, 0),
            ('scheduler_enabled', '1', 'scheduler', 'bool', 'Создавать задачу в планировщике', 'Проверять и запускать службу через Task Scheduler', 10, 0),
            ('scheduler_interval_minutes', '5', 'scheduler', 'int', 'Интервал проверки (мин)', 'Как часто планировщик проверяет состояние службы', 20, 0),
            ('agent_mode', 'hybrid', 'general', 'enum', 'Режим работы агента', 'local = только локально, hybrid = локально + облако', 5, 0),
            ('log_level', 'info', 'logs', 'enum', 'Уровень логирования', 'debug = всё, info = обычные события, warn = предупреждения, error = только ошибки', 10, 0),
            ('log_retention_days', '14', 'logs', 'int', 'Хранить логи (дней)', 'Логи старше этого срока удаляются автоматически', 20, 0);
            
            -- Пользователи
            CREATE TABLE IF NOT EXISTS users (
                sid TEXT PRIMARY KEY,
                username TEXT NOT NULL,
                is_controlled INTEGER DEFAULT 1,
                control_enabled INTEGER DEFAULT 1
            );

            -- Шаблон расписания на неделю
            CREATE TABLE IF NOT EXISTS weekly_schedule (
                user_sid TEXT NOT NULL,
                day_of_week INTEGER NOT NULL,
                limit_sec INTEGER DEFAULT 7200,
                internet_limit_sec INTEGER DEFAULT 7200,
                start_time TEXT DEFAULT '08:00',
                end_time TEXT DEFAULT '20:00',
                PRIMARY KEY (user_sid, day_of_week)
            );

            -- Рабочие лимиты на сегодня
            CREATE TABLE IF NOT EXISTS daily_limits (
                user_sid TEXT NOT NULL,
                date TEXT NOT NULL,
                limit_sec INTEGER DEFAULT 7200,
                internet_limit_sec INTEGER DEFAULT 7200,
                extra_time_min INTEGER DEFAULT 0,
                extra_internet_time_min INTEGER DEFAULT 0,
                start_time TEXT DEFAULT '',
                end_time TEXT DEFAULT '',
                mode TEXT DEFAULT 'scheduled',
                manual_until TEXT DEFAULT '',
                internet_paused INTEGER DEFAULT 0,
                internet_paused_source TEXT DEFAULT '',
                PRIMARY KEY (user_sid, date)
            );

            -- История использования
            CREATE TABLE IF NOT EXISTS daily_usage (
                user_sid TEXT NOT NULL,
                date TEXT NOT NULL,
                screen_time_sec INTEGER DEFAULT 0,
                internet_time_sec INTEGER DEFAULT 0,
                PRIMARY KEY (user_sid, date)
            );

            -- Индексы
            CREATE INDEX IF NOT EXISTS idx_daily_usage_user_date ON daily_usage(user_sid, date);
            CREATE INDEX IF NOT EXISTS idx_daily_limits_user_date ON daily_limits(user_sid, date);

            -- История блокировок
            CREATE TABLE IF NOT EXISTS lock_events (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                user_sid TEXT NOT NULL,
                locked_at TEXT NOT NULL,
                reason TEXT
            );
        ",
        )?;

        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// Конструктор по умолчанию: берёт путь из Config, если удалось загрузить,
    /// иначе — из DEFAULT_DB_DIR.
    pub fn new_default() -> Result<Self> {
        let dir = Config::load()
            .map(|c| c.paths.log_dir)
            .unwrap_or_else(|_| DEFAULT_DB_DIR.to_string());
        Self::new(dir)
    }
    // ===== Глобальные настройки =====
    // === Настройки (settings) ===

    /// Прочитать значение настройки
    pub fn get_setting(&self, key: &str) -> Result<Option<String>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT value FROM settings WHERE key = ?1")?;
        match stmt.query_row(params![key], |row| row.get(0)) {
            Ok(v) => Ok(Some(v)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// Записать значение настройки
    pub fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    pub fn get_settings(&self) -> Result<Vec<SettingItem>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT key, value, category, value_type, label, description, sort_order, is_secret
         FROM settings
         WHERE category != 'internal'         
         ORDER BY category, sort_order, key",
        )?;
        let items = stmt
            .query_map([], |row| {
                Ok(SettingItem {
                    key: row.get(0)?,
                    value: row.get(1)?,
                    category: row.get(2)?,
                    value_type: row.get(3)?,
                    label: row.get(4)?,
                    description: row.get(5)?,
                    sort_order: row.get(6)?,
                    is_secret: row.get(7)?,
                })
            })?
            .collect::<Result<Vec<_>>>()?;
        Ok(items)
    }
    // === Системные состояния (system_state) ===

    pub fn get_state(&self, key: &str) -> Result<Option<String>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT value FROM system_state WHERE key = ?1")?;
        match stmt.query_row(params![key], |row| row.get(0)) {
            Ok(v) => Ok(Some(v)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }

    pub fn set_state(&self, key: &str, value: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO system_state (key, value, updated_at)
         VALUES (?1, ?2, datetime('now'))
         ON CONFLICT(key) DO UPDATE SET
            value = ?2,
            updated_at = datetime('now')",
            params![key, value],
        )?;
        Ok(())
    }
    // ===== Пользователи =====
    pub fn get_all_users(&self) -> Result<Vec<(String, String)>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT sid, username FROM users")?;
        let users = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<Result<Vec<_>>>()?;
        Ok(users)
    }

    pub fn get_or_create_user(&self, sid: &str, username: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO users (sid, username) VALUES (?1, ?2)
             ON CONFLICT(sid) DO UPDATE SET username = ?2",
            params![sid, username],
        )?;

        for day in 0..7 {
            conn.execute(
                "INSERT OR IGNORE INTO weekly_schedule
                    (user_sid, day_of_week, limit_sec, internet_limit_sec, start_time, end_time)
                 VALUES (?1, ?2, 7200, 7200, '08:00', '20:00')",
                params![sid, day],
            )?;
        }
        Ok(())
    }

    pub fn get_username_by_sid(&self, sid: &str) -> Result<Option<String>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT username FROM users WHERE sid = ?1")?;
        let mut rows = stmt.query([sid])?;
        if let Some(row) = rows.next()? {
            Ok(Some(row.get(0)?))
        } else {
            Ok(None)
        }
    }

    /// Включён ли контроль для пользователя (флаг, не связан с лимитом)
    pub fn is_user_control_enabled(&self, sid: &str) -> Result<bool, rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT control_enabled FROM users WHERE sid = ?1")?;
        let enabled: i32 = stmt.query_row([sid], |row| row.get(0)).unwrap_or(1);
        Ok(enabled != 0)
    }

    /// Включает/выключает контроль. Только флаг, расписание НЕ трогает.
    pub fn set_user_control_enabled(
        &self,
        sid: &str,
        enabled: bool,
    ) -> Result<(), rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE users SET control_enabled = ?1 WHERE sid = ?2",
            rusqlite::params![if enabled { 1 } else { 0 }, sid],
        )?;
        Ok(())
    }

    /// Список пользователей с флагом контроля
    pub fn get_users_with_control_status(
        &self,
    ) -> Result<Vec<(String, String, bool)>, rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        let mut stmt =
            conn.prepare("SELECT sid, username, COALESCE(control_enabled, 1) FROM users")?;
        let users = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i32>(2)? != 0,
            ))
        })?;
        users.collect()
    }

    // ===== Расписание =====
    pub fn get_week_schedule(&self, sid: &str) -> Result<Vec<DaySchedule>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT day_of_week, limit_sec, internet_limit_sec, start_time, end_time
             FROM weekly_schedule WHERE user_sid = ?1 ORDER BY day_of_week",
        )?;
        let schedule = stmt
            .query_map(params![sid], |row| {
                Ok(DaySchedule {
                    day_of_week: row.get(0)?,
                    limit_sec: row.get(1)?,
                    internet_limit_sec: row.get(2)?,
                    start_time: row.get(3)?,
                    end_time: row.get(4)?,
                })
            })?
            .collect::<Result<Vec<_>>>()?;
        Ok(schedule)
    }

    pub fn get_day_schedule(&self, sid: &str, day_of_week: u32) -> Result<Option<DaySchedule>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT day_of_week, limit_sec, internet_limit_sec, start_time, end_time
             FROM weekly_schedule WHERE user_sid = ?1 AND day_of_week = ?2",
        )?;
        let result = stmt.query_row(params![sid, day_of_week], |row| {
            Ok(DaySchedule {
                day_of_week: row.get(0)?,
                limit_sec: row.get(1)?,
                internet_limit_sec: row.get(2)?,
                start_time: row.get(3)?,
                end_time: row.get(4)?,
            })
        });
        match result {
            Ok(schedule) => Ok(Some(schedule)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }

    pub fn update_day_schedule(&self, sid: &str, schedule: &DaySchedule) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE weekly_schedule
             SET limit_sec = ?1, internet_limit_sec = ?2, start_time = ?3, end_time = ?4
             WHERE user_sid = ?5 AND day_of_week = ?6",
            params![
                schedule.limit_sec,
                schedule.internet_limit_sec,
                schedule.start_time,
                schedule.end_time,
                sid,
                schedule.day_of_week
            ],
        )?;
        Ok(())
    }

    // ===== Лимиты на сегодня =====
    pub fn ensure_today_limit(&self, sid: &str) -> Result<TodayLimit> {
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();
        let today_dow = chrono::Local::now().weekday().num_days_from_sunday();

        let conn = self.conn.lock().unwrap();

        // 1. Пробуем прочитать существующий override на сегодня
        let mut stmt = conn.prepare(
            "SELECT limit_sec, internet_limit_sec, extra_time_min, extra_internet_time_min,
                    start_time, end_time, mode, manual_until
             FROM daily_limits
             WHERE user_sid = ?1 AND date = ?2",
        )?;

        let existing = stmt.query_row(params![sid, today], |row| {
            Ok(TodayLimit {
                limit_sec: row.get(0)?,
                internet_limit_sec: row.get(1)?,
                extra_time_min: row.get(2)?,
                extra_internet_time_min: row.get(3)?,
                start_time: row.get(4)?,
                end_time: row.get(5)?,
                mode: row.get(6)?,
                manual_until: row.get(7)?,
            })
        });

        match existing {
            Ok(limit) => Ok(limit),

            Err(rusqlite::Error::QueryReturnedNoRows) => {
                // 2. Override нет — берём базовые лимиты из расписания
                let mut stmt_schedule = conn.prepare(
                    "SELECT limit_sec, internet_limit_sec, start_time, end_time
                     FROM weekly_schedule
                     WHERE user_sid = ?1 AND day_of_week = ?2",
                )?;

                let schedule_result = stmt_schedule.query_row(params![sid, today_dow], |row| {
                    Ok((
                        row.get::<_, i32>(0)?,
                        row.get::<_, i32>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                    ))
                });

                let (limit_sec, internet_limit_sec, start_time, end_time) = match schedule_result {
                    Ok(vals) => vals,
                    Err(rusqlite::Error::QueryReturnedNoRows) => {
                        (7200, 7200, "08:00".to_string(), "20:00".to_string())
                    }
                    Err(e) => return Err(e),
                };

                let extra_time_min: i32 = 0;
                let extra_internet_time_min: i32 = 0;

                conn.execute(
                    "INSERT INTO daily_limits
                        (user_sid, date, limit_sec, internet_limit_sec,
                         extra_time_min, extra_internet_time_min,
                         start_time, end_time, mode, manual_until)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'scheduled', '')
                     ON CONFLICT(user_sid, date) DO UPDATE SET
                        limit_sec = ?3,
                        internet_limit_sec = ?4,
                        extra_time_min = ?5,
                        extra_internet_time_min = ?6,
                        start_time = ?7,
                        end_time = ?8",
                    params![
                        sid,
                        today,
                        limit_sec,
                        internet_limit_sec,
                        extra_time_min,
                        extra_internet_time_min,
                        start_time,
                        end_time
                    ],
                )?;

                Ok(TodayLimit {
                    limit_sec,
                    internet_limit_sec,
                    extra_time_min,
                    extra_internet_time_min,
                    start_time,
                    end_time,
                    mode: "scheduled".to_string(),
                    manual_until: String::new(),
                })
            }

            Err(e) => Err(e),
        }
    }

    pub fn set_today_limit(&self, sid: &str, limit: &TodayLimit) -> Result<()> {
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();
        let conn = self.conn.lock().unwrap();

        // mode/manual_until НЕ трогаем — они управляются отдельно
        conn.execute(
            "INSERT INTO daily_limits
            (user_sid, date,
             limit_sec, internet_limit_sec,
             extra_time_min, extra_internet_time_min,
             start_time, end_time)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
         ON CONFLICT(user_sid, date) DO UPDATE SET
            limit_sec = ?3,
            internet_limit_sec = ?4,
            extra_time_min = ?5,
            extra_internet_time_min = ?6,
            start_time = ?7,
            end_time = ?8",
            params![
                sid,
                today,
                limit.limit_sec,
                limit.internet_limit_sec,
                limit.extra_time_min,
                limit.extra_internet_time_min,
                limit.start_time,
                limit.end_time
            ],
        )?;
        Ok(())
    }

    /// Сбрасывает режим manual_lock/manual_unlock → scheduled для сегодняшнего TodayLimit.
    /// Возвращает `true`, если реально был manual и его сбросили.
    pub fn return_to_scheduled(&self, sid: &str) -> Result<bool> {
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();
        let conn = self.conn.lock().unwrap();

        let rows = conn.execute(
            "UPDATE daily_limits
         SET mode = 'scheduled', manual_until = ''
         WHERE user_sid = ?1 AND date = ?2
           AND mode IN ('manual_lock', 'manual_unlock')",
            params![sid, today],
        )?;

        Ok(rows > 0)
    }

    pub fn add_extra_time(&self, sid: &str, minutes: i32) -> Result<()> {
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO daily_limits (user_sid, date, extra_time_min)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(user_sid, date) DO UPDATE SET
                extra_time_min = extra_time_min + ?3",
            params![sid, today, minutes],
        )?;
        Ok(())
    }

    pub fn add_extra_internet_time(&self, sid: &str, minutes: i32) -> Result<()> {
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO daily_limits (user_sid, date, extra_internet_time_min)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(user_sid, date) DO UPDATE SET
                extra_internet_time_min = extra_internet_time_min + ?3",
            params![sid, today, minutes],
        )?;
        Ok(())
    }

    // ===== Режим на сегодня (scheduled / manual) =====
    pub fn set_today_mode(&self, sid: &str, mode: &str, manual_until: &str) -> Result<()> {
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO daily_limits (user_sid, date, mode, manual_until)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(user_sid, date) DO UPDATE SET
                mode = ?3,
                manual_until = ?4",
            params![sid, today, mode, manual_until],
        )?;
        Ok(())
    }

    pub fn get_today_mode(&self, sid: &str) -> Result<(String, String)> {
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT mode, manual_until FROM daily_limits WHERE user_sid = ?1 AND date = ?2",
        )?;
        let result = stmt.query_row(params![sid, today], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        });
        match result {
            Ok(v) => Ok(v),
            Err(rusqlite::Error::QueryReturnedNoRows) => {
                Ok(("scheduled".to_string(), String::new()))
            }
            Err(e) => Err(e),
        }
    }

    /// Возвращает режим в "scheduled", пересчитывая лимиты из расписания.
    pub fn restore_limit_from_schedule(&self, sid: &str) -> Result<()> {
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();
        let today_dow = chrono::Local::now().weekday().num_days_from_sunday();

        let conn = self.conn.lock().unwrap();

        let mut stmt = conn.prepare(
            "SELECT limit_sec, internet_limit_sec, start_time, end_time
             FROM weekly_schedule
             WHERE user_sid = ?1 AND day_of_week = ?2",
        )?;

        let row = stmt.query_row(params![sid, today_dow], |row| {
            Ok((
                row.get::<_, i32>(0)?,
                row.get::<_, i32>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        });

        let (limit_sec, internet_limit_sec, start_time, end_time) = match row {
            Ok(v) => v,
            Err(rusqlite::Error::QueryReturnedNoRows) => {
                (7200, 7200, "08:00".to_string(), "20:00".to_string())
            }
            Err(e) => return Err(e),
        };

        conn.execute(
            "INSERT INTO daily_limits
                (user_sid, date, limit_sec, internet_limit_sec, start_time, end_time,
                 mode, manual_until)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'scheduled', '')
             ON CONFLICT(user_sid, date) DO UPDATE SET
                limit_sec = ?3,
                internet_limit_sec = ?4,
                start_time = ?5,
                end_time = ?6,
                mode = 'scheduled',
                manual_until = ''",
            params![
                sid,
                today,
                limit_sec,
                internet_limit_sec,
                start_time,
                end_time
            ],
        )?;
        Ok(())
    }

    // ===== Статистика =====
    pub fn add_screen_time(&self, sid: &str, seconds: u32) -> Result<()> {
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO daily_usage (user_sid, date, screen_time_sec)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(user_sid, date) DO UPDATE SET
                screen_time_sec = screen_time_sec + ?3",
            params![sid, today, seconds],
        )?;
        Ok(())
    }

    pub fn add_internet_time(&self, sid: &str, seconds: u32) -> Result<()> {
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO daily_usage (user_sid, date, internet_time_sec)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(user_sid, date) DO UPDATE SET
                internet_time_sec = internet_time_sec + ?3",
            params![sid, today, seconds],
        )?;
        Ok(())
    }

    pub fn get_today_usage(&self, sid: &str) -> Result<(u32, u32)> {
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT screen_time_sec, internet_time_sec
             FROM daily_usage WHERE user_sid = ?1 AND date = ?2",
        )?;
        let result = stmt.query_row(params![sid, today], |row| {
            Ok((row.get(0).unwrap_or(0), row.get(1).unwrap_or(0)))
        });
        match result {
            Ok(val) => Ok(val),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok((0, 0)),
            Err(e) => Err(e),
        }
    }

    // ===== События блокировки =====
    pub fn log_lock_event(&self, sid: &str, reason: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let locked_at = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        conn.execute(
            "INSERT INTO lock_events (user_sid, locked_at, reason) VALUES (?1, ?2, ?3)",
            params![sid, locked_at, reason],
        )?;
        Ok(())
    }

    // ===== Очистка устаревших пользователей =====
    pub fn cleanup_stale_users(&self, active_sids: &[String]) -> Result<usize> {
        if active_sids.is_empty() {
            return Ok(0);
        }
        let conn = self.conn.lock().unwrap();
        let mut deactivated_count = 0;
        let mut stmt = conn.prepare("SELECT sid FROM users")?;
        let db_sids: Vec<String> = stmt
            .query_map([], |row| row.get(0))?
            .filter_map(Result::ok)
            .collect();
        for db_sid in db_sids {
            if !active_sids.contains(&db_sid) {
                // ⚠️ Колонки is_active в схеме нет — оставляем только подсчёт.
                // Если нужно реально деактивировать — добавь колонку is_active в users.
                deactivated_count += 1;
            }
        }
        Ok(deactivated_count)
    }

    // ===== Пауза интернета =====

    /// Устанавливает флаг паузы интернета на сегодня с указанием источника.
    /// `source` — "manual" (родитель) или "limit" (по лимиту) или "".
    pub fn set_internet_paused(&self, sid: &str, paused: bool, source: &str) -> Result<()> {
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();
        let conn = self.conn.lock().unwrap();
        let val: i32 = if paused { 1 } else { 0 };

        conn.execute(
            "INSERT INTO daily_limits
            (user_sid, date, internet_paused, internet_paused_source)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(user_sid, date) DO UPDATE SET
            internet_paused = ?3,
            internet_paused_source = ?4",
            params![sid, today, val, source],
        )?;
        Ok(())
    }
    /// Читает источник паузы интернета на сегодня.
    /// "" — пауза не установлена.
    /// "manual" — родитель нажал кнопку.
    /// "limit" — система заблокировала по лимиту.
    pub fn get_internet_paused_source(&self, sid: &str) -> Result<String> {
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT internet_paused_source FROM daily_limits
         WHERE user_sid = ?1 AND date = ?2",
        )?;
        match stmt.query_row(params![sid, today], |row| row.get::<_, String>(0)) {
            Ok(s) => Ok(s),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(String::new()),
            Err(e) => Err(e),
        }
    }
    /// Читает флаг паузы интернета на сегодня.
    pub fn is_internet_paused(&self, sid: &str) -> Result<bool> {
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT internet_paused FROM daily_limits
         WHERE user_sid = ?1 AND date = ?2",
        )?;
        match stmt.query_row(params![sid, today], |row| row.get::<_, i32>(0)) {
            Ok(v) => Ok(v != 0),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(false),
            Err(e) => Err(e),
        }
    }

    pub fn get_setting_meta(&self, key: &str) -> Result<Option<SettingMeta>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT value_type, label, description, category
         FROM settings WHERE key = ?1",
        )?;
        match stmt.query_row(params![key], |row| {
            Ok(SettingMeta {
                value_type: row.get(0)?,
                label: row.get(1)?,
                description: row.get(2)?,
                category: row.get(3)?,
            })
        }) {
            Ok(m) => Ok(Some(m)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }
}
