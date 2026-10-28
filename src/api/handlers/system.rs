use axum::{extract::State, Json};

use crate::error::AppResult;

use super::super::{dto::{DayDefaults, DefaultsResponse, WeekDayInfo}, AppState};

pub async fn health_check(State(state): State<AppState>) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "status": "ok",
        "service": state.config.service.name,
    }))
}

pub async fn get_defaults(State(state): State<AppState>) -> AppResult<Json<DefaultsResponse>> {
    let limits = &state.config.limits;
    let day = DayDefaults {
        start_time: limits.default_start_time.clone(),
        end_time: limits.default_end_time.clone(),
        limit_min: (limits.default_limit_sec / 60) as i32,
        internet_limit_min: (limits.default_limit_sec / 60) as i32,
    };

    // Порядок как на фронте: Пн..Вс. day_of_week: 1,2,3,4,5,6,0.
    let names = [
        "Понедельник", "Вторник", "Среда", "Четверг",
        "Пятница", "Суббота", "Воскресенье",
    ];
    let week = names
        .iter()
        .enumerate()
        .map(|(fe_idx, name)| WeekDayInfo {
            day_of_week: if fe_idx == 6 { 0 } else { (fe_idx + 1) as u32 },
            name: (*name).to_string(),
            frontend_idx: fe_idx,
        })
        .collect();

    Ok(Json(DefaultsResponse { day, week }))
}