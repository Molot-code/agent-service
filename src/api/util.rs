use chrono::{Local, NaiveDateTime};
use serde_json::json;

use crate::{db::AppDb, error::AppError, firewall, log_info};

use super::AppState;

pub fn device_name(state: &AppState) -> String {
    state
        .db
        .get_setting("device_name")
        .ok()
        .flatten()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| {
            std::env::var("COMPUTERNAME").unwrap_or_else(|_| "Unknown".to_string())
        })
}

pub fn resolve_manual_until(input: &str) -> Option<String> {
    if input == "-1" {
        let eod = Local::now().date_naive().and_hms_opt(23, 59, 59)?;
        return Some(eod.format("%Y-%m-%d %H:%M:%S").to_string());
    }
    NaiveDateTime::parse_from_str(input, "%Y-%m-%d %H:%M:%S")
        .ok()
        .map(|_| input.to_string())
}

/// Применяет extra_time (screen и/или internet) с флагом add/set.
pub fn apply_extra(
    db: &AppDb,
    sid: &str,
    screen: Option<i32>,
    internet: Option<i32>,
    additive: bool,
) -> crate::error::AppResult<()> {
    if screen.is_none() && internet.is_none() {
        return Err(AppError::BadRequest("nothing to update".into()));
    }
    if let Some(m) = screen {
        if additive { db.add_extra_time(sid, m)?; } else { db.set_extra_time(sid, m)?; }
    }
    if let Some(m) = internet {
        if additive { db.add_extra_internet_time(sid, m)?; } else { db.set_extra_internet_time(sid, m)?; }
    }
    Ok(())
}

/// Управление блокировкой интернета: firewall + флаги в БД + лог.
pub fn set_internet_blocked(
    db: &AppDb,
    sid: &str,
    blocked: bool,
    source: &str,
) -> crate::error::AppResult<()> {
    if blocked {
        firewall::ensure_block_rule(sid)
            .map_err(|e| AppError::Internal(format!("firewall ensure: {e}")))?;
        firewall::set_block_enabled(sid, true)
            .map_err(|e| AppError::Internal(format!("firewall enable: {e}")))?;
    } else {
        firewall::set_block_enabled(sid, false)
            .map_err(|e| AppError::Internal(format!("firewall disable: {e}")))?;
    }
    db.set_internet_paused(sid, blocked, source)?;
    db.set_state("internet_blocked", if blocked { "true" } else { "false" })?;
    log_info!(
        "[API] Интернет {} для {}",
        if blocked { "на паузе" } else { "возобновлён" },
        sid
    );
    Ok(())
}

pub fn extra_response(sid: &str, screen: Option<i32>, internet: Option<i32>) -> serde_json::Value {
    json!({
        "success": true,
        "sid": sid,
        "screen_minutes": screen,
        "internet_minutes": internet,
    })
}