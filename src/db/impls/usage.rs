use chrono::Local;
use rusqlite::{params, Result};

use super::super::AppDb;

#[derive(Debug, Clone, serde::Serialize)]
pub struct ActivityLogEntry {
    pub id: i64,
    pub event_type: String,
    pub details: String,
    pub screen_time_sec: i32,
    pub internet_time_sec: i32,
    pub screen_limit_sec: i32,
    pub internet_limit_sec: i32,
    pub is_locked: i32,
    pub internet_paused: i32,
    pub created_at: String,
    pub state: String,
}

fn today() -> String {
    Local::now().format("%Y-%m-%d").to_string()
}

impl AppDb {
    pub fn add_screen_time(&self, sid: &str, seconds: u32) -> Result<()> {
        self.bump_usage(sid, "screen_time_sec", seconds)
    }

    pub fn add_internet_time(&self, sid: &str, seconds: u32) -> Result<()> {
        self.bump_usage(sid, "internet_time_sec", seconds)
    }

    fn bump_usage(&self, sid: &str, col: &'static str, seconds: u32) -> Result<()> {
        let sql = format!(
            "INSERT INTO daily_usage (user_sid, date, {col}) VALUES (?1, ?2, ?3)
             ON CONFLICT(user_sid, date) DO UPDATE SET {col} = {col} + ?3"
        );
        let conn = self.conn.lock().unwrap();
        conn.execute(&sql, params![sid, today(), seconds])?;
        Ok(())
    }

    pub fn get_today_usage(&self, sid: &str) -> Result<(u32, u32)> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare_cached(
            "SELECT screen_time_sec, internet_time_sec
             FROM daily_usage WHERE user_sid = ?1 AND date = ?2",
        )?;
        match stmt.query_row(params![sid, today()], |r| {
            Ok((r.get(0).unwrap_or(0), r.get(1).unwrap_or(0)))
        }) {
            Ok(v) => Ok(v),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok((0, 0)),
            Err(e) => Err(e),
        }
    }

    pub fn log_lock_event(&self, sid: &str, reason: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let ts = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        conn.execute(
            "INSERT INTO lock_events (user_sid, locked_at, reason) VALUES (?1, ?2, ?3)",
            params![sid, ts, reason],
        )?;
        Ok(())
    }

    pub fn log_activity(
        &self,
        sid: &str,
        event_type: &str,
        details: &str,
        screen_time: i64,
        internet_time: i64,
        screen_limit: i64,
        internet_limit: i64,
        is_locked: bool,
        internet_paused: bool,
        state: &str, // 🆕
    ) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO activity_log
                (user_sid, event_type, details,
                screen_time_sec, internet_time_sec,
                screen_limit_sec, internet_limit_sec,
                is_locked, internet_paused, state, created_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, datetime('now'))",
            params![
                sid,
                event_type,
                details,
                screen_time,
                internet_time,
                screen_limit,
                internet_limit,
                is_locked as i32,
                internet_paused as i32,
                state
            ],
        )?;
        Ok(())
    }

    pub fn get_activity_log(
        &self,
        sid: &str,
        date: Option<&str>,
        limit: i32,
    ) -> Result<Vec<ActivityLogEntry>> {
        let conn = self.conn.lock().unwrap();

        // Если дата передана — фильтруем по DATE(created_at) = date,
        // иначе — последние N записей по всем дням.
        let rows = if let Some(day) = date {
            let mut stmt = conn.prepare_cached(
                "SELECT id, event_type, details, screen_time_sec, internet_time_sec,
                    screen_limit_sec, internet_limit_sec, is_locked, internet_paused, created_at, state
             FROM activity_log
             WHERE user_sid = ?1 AND DATE(created_at, 'localtime') = ?2
             ORDER BY created_at ASC
             LIMIT ?3",
            )?;
            let iter = stmt.query_map(params![sid, day, limit], |r| {
                Ok(ActivityLogEntry {
                    id: r.get(0)?,
                    event_type: r.get(1)?,
                    details: r.get(2)?,
                    screen_time_sec: r.get(3)?,
                    internet_time_sec: r.get(4)?,
                    screen_limit_sec: r.get(5)?,
                    internet_limit_sec: r.get(6)?,
                    is_locked: r.get(7)?,
                    internet_paused: r.get(8)?,
                    created_at: r.get(9)?,
                    state: r.get(10)?, // 🆕
                })
            })?;
            iter.collect::<Result<Vec<_>, _>>()?
        } else {
            let mut stmt = conn.prepare_cached(
                "SELECT id, event_type, details, screen_time_sec, internet_time_sec,
                    screen_limit_sec, internet_limit_sec, is_locked, internet_paused, created_at, state
             FROM activity_log
             WHERE user_sid = ?1
             ORDER BY created_at DESC
             LIMIT ?2",
            )?;
            let iter = stmt.query_map(params![sid, limit], |r| {
                Ok(ActivityLogEntry {
                    id: r.get(0)?,
                    event_type: r.get(1)?,
                    details: r.get(2)?,
                    screen_time_sec: r.get(3)?,
                    internet_time_sec: r.get(4)?,
                    screen_limit_sec: r.get(5)?,
                    internet_limit_sec: r.get(6)?,
                    is_locked: r.get(7)?,
                    internet_paused: r.get(8)?,
                    created_at: r.get(9)?,
                    state: r.get(10)?, // 🆕
                })
            })?;
            iter.collect::<Result<Vec<_>, _>>()?
        };

        Ok(rows)
    }
}
