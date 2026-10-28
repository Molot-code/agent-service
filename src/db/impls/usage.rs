use chrono::Local;
use rusqlite::{params, Result};

use super::super::AppDb;

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
}