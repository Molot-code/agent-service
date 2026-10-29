use std::{env, sync::Arc};

use tokio::sync::mpsc;

use super::{
    router,
    state::{init_api_token, init_tray_token},
    AppState,
};
use crate::{config::Config, db::AppDb, log_debug, log_error, log_info, log_warn, AdminCommand};

pub async fn start_api_server(
    db: Arc<AppDb>,
    config: Arc<Config>,
    admin_cmd_tx: mpsc::Sender<AdminCommand>,
) {
    log_debug!("🔌 [API] start_api_server");

    let Some(api_token) = init_api_token(&db) else {
        return;
    };
    let Some(_tray_token) = init_tray_token(&db, &config.paths.data_dir) else {
        return;
    };

    let exe_dir = match env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
    {
        Some(p) => p,
        None => {
            log_error!("❌ [API] Не удалось получить путь к exe");
            return;
        }
    };

    let templates_dir = exe_dir.join("templates");
    if templates_dir.exists() {
        log_debug!("✅ [API] templates: {:?}", templates_dir);
    } else {
        log_warn!("⚠️ [API] templates НЕ найдена: {:?}", templates_dir);
    }

    let mut env_builder = minijinja::Environment::new();
    env_builder.set_loader(minijinja::path_loader(templates_dir));
    let env = Arc::new(env_builder);

    let static_dir = exe_dir.join(&config.paths.static_dir);
    if static_dir.exists() {
        log_debug!("✅ [API] static: {:?}", static_dir);
    } else {
        log_warn!("⚠️ [API] static НЕ найдена: {:?}", static_dir);
    }

    let state = AppState {
        db,
        api_token,
        admin_cmd_tx,
        env,
        config: Arc::clone(&config),
    };

    let app = router::build(static_dir, state);

    let addr = format!("0.0.0.0:{}", config.service.api_port);
    log_info!("📡 [API] bind: {}", addr);

    match tokio::net::TcpListener::bind(&addr).await {
        Ok(listener) => {
            log_info!("🚀 [API] запущен на http://{}", addr);
            if let Err(e) = axum::serve(listener, app).await {
                log_error!("❌ [API] сервер упал: {}", e);
            }
        }
        Err(e) => log_error!("❌ [API] не удалось занять порт {}: {}", addr, e),
    }
}
