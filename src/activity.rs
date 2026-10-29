use std::sync::Arc;

use crate::db::AppDb;

pub fn current_state(db: &Arc<AppDb>, sid: &str) -> &'static str {
    if db.get_state("lock_status").ok().flatten().as_deref() == Some("true") {
        return "screen_locked";
    }
    if db.get_state("session_locked").ok().flatten().as_deref() == Some("true") {
        return "session_locked";
    }

    let mode = db.get_today_mode(sid).map(|(m, _)| m).unwrap_or_default();
    let control_on = db.is_user_control_enabled(sid).unwrap_or(true);
    if !control_on || matches!(mode.as_str(), "manual_lock" | "manual_unlock") {
        return "active_manual";
    }

    if db.is_internet_paused(sid).unwrap_or(false) {
        return "no_internet";
    }
    "active_scheduled"
}

pub fn log_activity_now(db: &Arc<AppDb>, sid: &str, event: &str, details: &str) {
    let (screen_time, internet_time) = db.get_today_usage(sid).unwrap_or((0, 0));
    let today_limit = db.ensure_today_limit(sid).unwrap_or_default();
    let state = current_state(db, sid);
    let is_locked = matches!(state, "screen_locked" | "session_locked");
    let internet_paused = matches!(state, "no_internet") || is_locked;

    let _ = db.log_activity(
        sid,
        event,
        details,
        screen_time.into(),
        internet_time.into(),
        today_limit.limit_sec.into(),
        today_limit.internet_limit_sec.into(),
        is_locked,
        internet_paused,
        state,
    );
}
