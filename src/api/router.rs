use axum::{
    middleware,
    routing::{get, post},
    Router,
};
use tower_http::services::ServeDir;

use super::{
    auth::{require_admin, require_auth},
    handlers::{control, pin, system, web},
    AppState,
};

pub fn build(static_dir: std::path::PathBuf, state: AppState) -> Router {
    // Группа, требующая хотя бы Tray.
    let tray_api = Router::new()
        .route("/api/status", get(web::get_status))
        .route("/api/users", get(web::get_users))
        .route(
            "/api/schedule/{sid}",
            get(web::get_week_schedule).post(web::save_week_schedule),
        )
        .route(
            "/api/today/{sid}",
            get(web::get_today_limit).post(web::save_today_limit),
        )
        .route("/api/settings", get(web::get_settings))
        .route("/api/logs", get(web::list_log_dates))
        .route("/api/logs/{date}", get(web::get_logs))
        .route("/api/internet/status", get(control::internet_status))
        .route_layer(middleware::from_fn_with_state(state.clone(), require_auth))
        .route("/api/defaults", get(system::get_defaults));

    // Группа, требующая Admin (api_token или admin_pin).
    let admin_api = Router::new()
        .route("/api/set_param", post(web::set_param))
        .route("/api/manual_action", post(control::manual_action))
        .route("/api/extra_time", post(control::extra_time))
        .route("/api/set_extra_time", post(control::set_extra_time))
        .route("/api/users/toggle_control", post(control::toggle_user_control))
        .route("/api/internet/pause", post(control::pause_internet))
        .route("/api/internet/resume", post(control::resume_internet))
        .route_layer(middleware::from_fn_with_state(state.clone(), require_admin));

    // PIN-эндпоинты доступны без авторизации (трей до логина).
    let pin_api = Router::new()
        .route("/api/check_pin", post(pin::check_pin))
        .route("/api/set_pin", post(pin::set_admin_pin))
        .route("/api/is_pin_set", get(pin::is_pin_set));

    Router::new()
        .route("/", get(web::index_page))
        .route("/health", get(system::health_check))
        .route(
            "/switch-server",
            get(|| async { axum::response::Redirect::to("/") }),
        )
        .merge(tray_api)
        .merge(admin_api)
        .merge(pin_api)
        .fallback_service(ServeDir::new(static_dir).append_index_html_on_directories(true))
        .with_state(state)
}