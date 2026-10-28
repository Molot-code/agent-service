use chrono::{Datelike, Local};
use rusqlite::{params, Result};

use crate::types::TodayLimit;

use super::super::AppDb;

// ---------- helpers ----------

fn today() -> String {
    Local::now().format("%Y-%m-%d").to_string()
}

fn today_dow() -> u32 {
    Local::now().weekday().num_days_from_sunday()
}

/// Возвращает (limit_sec, internet_limit_sec, start_time, end_time) из расписания
/// на сегодня, либо дефолт при отсутствии записи.
fn read_schedule_for_today(
    conn: &rusqlite::Connection,
    sid: &str,
    dow: u32,
) -> Result<(i32, i32, String, String)> {
    let mut stmt = conn.prepare_cached(
        "SELECT limit_sec, internet_limit_sec, start_time, end_time
         FROM weekly_schedule WHERE user_sid = ?1 AND day_of_week = ?2",
    )?;
    match stmt.query_row(params![sid, dow], |row| {
        Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
    }) {
        Ok(v) => Ok(v),
        Err(rusqlite::Error::QueryReturnedNoRows) => {
            Ok((7200, 7200, "08:00".into(), "20:00".into()))
        }
        Err(e) => Err(e),
    }
}

/// Что «дополнять/устанавливать» в daily_limits.
enum ExtraKind {
    Screen,
    Internet,
}

impl ExtraKind {
    fn column(self) -> &'static str {
        match self {
            Self::Screen => "extra_time_min",
            Self::Internet => "extra_internet_time_min",
        }
    }
}

// ---------- today limit ----------

impl AppDb {
    pub fn ensure_today_limit(&self, sid: &str) -> Result<TodayLimit> {
        let today = today();
        let conn = self.conn.lock().unwrap();

        // 1. override на сегодня?
        let existing = {
            let mut stmt = conn.prepare_cached(
                "SELECT limit_sec, internet_limit_sec, extra_time_min, extra_internet_time_min,
                        start_time, end_time, mode, manual_until
                 FROM daily_limits WHERE user_sid = ?1 AND date = ?2",
            )?;
            stmt.query_row(params![sid, today], |row| {
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
            })
        };
        match existing {
            Ok(l) => return Ok(l),
            Err(rusqlite::Error::QueryReturnedNoRows) => {}
            Err(e) => return Err(e),
        }

        // 2. override нет — берём из расписания
        let (limit_sec, internet_limit_sec, start_time, end_time) =
            read_schedule_for_today(&conn, sid, today_dow())?;

        conn.execute(
            "INSERT INTO daily_limits
                (user_sid, date, limit_sec, internet_limit_sec,
                 extra_time_min, extra_internet_time_min,
                 start_time, end_time, mode, manual_until)
             VALUES (?1, ?2, ?3, ?4, 0, 0, ?5, ?6, 'scheduled', '')
             ON CONFLICT(user_sid, date) DO UPDATE SET
                limit_sec = ?3, internet_limit_sec = ?4,
                start_time = ?5, end_time = ?6",
            params![sid, today, limit_sec, internet_limit_sec, start_time, end_time],
        )?;

        Ok(TodayLimit {
            limit_sec,
            internet_limit_sec,
            extra_time_min: 0,
            extra_internet_time_min: 0,
            start_time,
            end_time,
            mode: "scheduled".to_string(),
            manual_until: String::new(),
        })
    }

    pub fn set_today_limit(&self, sid: &str, limit: &TodayLimit) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        // mode/manual_until не трогаем — ими управляют set_today_mode/return_to_scheduled.
        conn.execute(
            "INSERT INTO daily_limits
                (user_sid, date, limit_sec, internet_limit_sec,
                 extra_time_min, extra_internet_time_min, start_time, end_time)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(user_sid, date) DO UPDATE SET
                limit_sec = ?3, internet_limit_sec = ?4,
                extra_time_min = ?5, extra_internet_time_min = ?6,
                start_time = ?7, end_time = ?8",
            params![
                sid, today(),
                limit.limit_sec, limit.internet_limit_sec,
                limit.extra_time_min, limit.extra_internet_time_min,
                limit.start_time, limit.end_time
            ],
        )?;
        Ok(())
    }

    pub fn return_to_scheduled(&self, sid: &str) -> Result<bool> {
        let conn = self.conn.lock().unwrap();
        let rows = conn.execute(
            "UPDATE daily_limits
             SET mode = 'scheduled', manual_until = ''
             WHERE user_sid = ?1 AND date = ?2
               AND mode IN ('manual_lock', 'manual_unlock')",
            params![sid, today()],
        )?;
        Ok(rows > 0)
    }

    pub fn restore_limit_from_schedule(&self, sid: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let (limit_sec, internet_limit_sec, start_time, end_time) =
            read_schedule_for_today(&conn, sid, today_dow())?;

        conn.execute(
            "INSERT INTO daily_limits
                (user_sid, date, limit_sec, internet_limit_sec, start_time, end_time,
                 mode, manual_until)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'scheduled', '')
             ON CONFLICT(user_sid, date) DO UPDATE SET
                limit_sec = ?3, internet_limit_sec = ?4,
                start_time = ?5, end_time = ?6,
                mode = 'scheduled', manual_until = ''",
            params![sid, today(), limit_sec, internet_limit_sec, start_time, end_time],
        )?;
        Ok(())
    }
}

// ---------- extra time ----------

impl AppDb {
    pub fn add_extra_time(&self, sid: &str, minutes: i32) -> Result<()> {
        self.mutate_extra(sid, ExtraKind::Screen, minutes, true)
    }

    pub fn set_extra_time(&self, sid: &str, minutes: i32) -> Result<()> {
        self.mutate_extra(sid, ExtraKind::Screen, minutes, false)
    }

    pub fn add_extra_internet_time(&self, sid: &str, minutes: i32) -> Result<()> {
        self.mutate_extra(sid, ExtraKind::Internet, minutes, true)
    }

    pub fn set_extra_internet_time(&self, sid: &str, minutes: i32) -> Result<()> {
        self.mutate_extra(sid, ExtraKind::Internet, minutes, false)
    }

    fn mutate_extra(&self, sid: &str, kind: ExtraKind, minutes: i32, additive: bool) -> Result<()> {
        let col = kind.column();
        let op = if additive { format!("{col} + ?3") } else { "?3".to_string() };
        let sql = format!(
            "INSERT INTO daily_limits (user_sid, date, {col}) VALUES (?1, ?2, ?3)
             ON CONFLICT(user_sid, date) DO UPDATE SET {col} = {op}"
        );
        let conn = self.conn.lock().unwrap();
        conn.execute(&sql, params![sid, today(), minutes])?;
        Ok(())
    }
}

// ---------- mode ----------

impl AppDb {
    pub fn set_today_mode(&self, sid: &str, mode: &str, manual_until: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO daily_limits (user_sid, date, mode, manual_until)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(user_sid, date) DO UPDATE SET mode = ?3, manual_until = ?4",
            params![sid, today(), mode, manual_until],
        )?;
        Ok(())
    }

    pub fn get_today_mode(&self, sid: &str) -> Result<(String, String)> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare_cached(
            "SELECT mode, manual_until FROM daily_limits WHERE user_sid = ?1 AND date = ?2",
        )?;
        match stmt.query_row(params![sid, today()], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        }) {
            Ok(v) => Ok(v),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(("scheduled".into(), String::new())),
            Err(e) => Err(e),
        }
    }
}

// ---------- internet pause ----------

impl AppDb {
    pub fn set_internet_paused(&self, sid: &str, paused: bool, source: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO daily_limits
                (user_sid, date, internet_paused, internet_paused_source)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(user_sid, date) DO UPDATE SET
                internet_paused = ?3, internet_paused_source = ?4",
            params![sid, today(), paused as i32, source],
        )?;
        Ok(())
    }

    pub fn get_internet_paused_source(&self, sid: &str) -> Result<String> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare_cached(
            "SELECT internet_paused_source FROM daily_limits
             WHERE user_sid = ?1 AND date = ?2",
        )?;
        match stmt.query_row(params![sid, today()], |r| r.get::<_, String>(0)) {
            Ok(s) => Ok(s),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(String::new()),
            Err(e) => Err(e),
        }
    }

    pub fn is_internet_paused(&self, sid: &str) -> Result<bool> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare_cached(
            "SELECT internet_paused FROM daily_limits
             WHERE user_sid = ?1 AND date = ?2",
        )?;
        match stmt.query_row(params![sid, today()], |r| r.get::<_, i32>(0)) {
            Ok(v) => Ok(v != 0),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(false),
            Err(e) => Err(e),
        }
    }
}