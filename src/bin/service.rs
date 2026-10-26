// src/bin/service.rs
use std::{env, path::Path, sync::Arc, time::Duration};

use agent_windows::{log_debug, log_error, log_info, policy::do_sync_user_access, scheduler};
use chrono::Local;
use tokio::sync::{mpsc, watch};
use windows::Win32::System::RemoteDesktop::{
    WTSFreeMemory, WTSGetActiveConsoleSessionId, WTSQuerySessionInformationW, WTS_INFO_CLASS,
};
use windows_service::{
    define_windows_service,
    // В версии 0.8.1 абсолютно все типы, связанные со свойствами службы, лежат здесь:
    service::{
        ServiceAccess, ServiceControl, ServiceControlAccept, ServiceErrorControl, ServiceExitCode,
        ServiceInfo, ServiceStartType, ServiceState, ServiceStatus, ServiceType,
    },
    service_control_handler::{self, ServiceControlHandlerResult},
    service_dispatcher,
    // Отсюда нам нужен только сам менеджер
    service_manager::{ServiceManager, ServiceManagerAccess},
};

use crate::users::get_active_user_sid;
// Определяем событие сессии (можно вынести в types.rs, если хотите)
#[derive(Debug, Clone)]
pub enum SessionEvent {
    Lock(u32),
    Unlock(u32),
    Stop,
}

use agent_windows::{
    api::start_api_server,
    cloud_client::start_cloud_client,
    config::Config,
    db::AppDb,
    discovery::start_discovery_listener,
    logger::{init_global_logger, LogLevel},
    mode::get_current_mode,
    policy::{evaluate_access, PolicyCheckResult},
    types::ControlMode,
    users, AdminCommand, SessionState,
};
// ==========================================
// 2. ТОЧКИ ВХОДА WINDOWS SERVICE
// ==========================================
define_windows_service!(ffi_service_main, service_main);

fn service_main(arguments: Vec<std::ffi::OsString>) {
    if let Err(e) = run_service(arguments) {
        log_to_file(&format!("Критическая ошибка службы: {}", e));
    }
}

fn run_service(_arguments: Vec<std::ffi::OsString>) -> Result<(), Box<dyn std::error::Error>> {
    // 1. Конфиг — нужен log_dir для логгера
    let config = Arc::new(Config::load().map_err(|e| {
        eprintln!("❌ Не удалось загрузить config.toml: {}", e);
        e
    })?);

    // 2. Инициализация глобального логгера
    let logs_path = Path::new(&config.paths.log_dir).join("logs");
    init_global_logger(
        &logs_path.to_string_lossy(),
        "service",
        LogLevel::from_str(&config.service.log_level),
        14,
    );

    log_info!(
        "✅ Конфигурация загружена: порт {}",
        config.service.api_port
    );

    // 3. Канал событий сессии
    let (session_event_tx, mut session_event_rx) = tokio::sync::mpsc::channel::<SessionEvent>(100);

    // 4. Обработчик системных сигналов SCM
    let tx_clone = session_event_tx.clone();
    let event_handler = move |control_event| -> ServiceControlHandlerResult {
        use windows_service::service::SessionChangeReason as R;

        log_info!(
            "🚨 [SCM Handler] ПОЛУЧЕНО ЛЮБОЕ СОБЫТИЕ: {:?}",
            control_event
        );

        match control_event {
            ServiceControl::SessionChange(change) => {
                let sid = change.notification.session_id;
                let event = match change.reason {
                    R::SessionLock | R::RemoteDisconnect | R::ConsoleDisconnect => {
                        Some(SessionEvent::Lock(sid))
                    }
                    R::SessionUnlock | R::RemoteConnect | R::ConsoleConnect => {
                        Some(SessionEvent::Unlock(sid))
                    }
                    _ => None,
                };

                if let Some(ev) = event {
                    let _ = tx_clone.try_send(ev);
                }
                ServiceControlHandlerResult::NoError
            }

            ServiceControl::Stop | ServiceControl::Shutdown => {
                log_info!("🛑 [SCM Handler] Служба останавливается...");
                let _ = tx_clone.try_send(SessionEvent::Stop);
                ServiceControlHandlerResult::NoError
            }

            ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
            _ => ServiceControlHandlerResult::NotImplemented,
        }
    };

    // 5. Регистрация службы
    let status_handle = service_control_handler::register(&config.service.name, event_handler)?;

    status_handle.set_service_status(ServiceStatus {
        service_type: ServiceType::OWN_PROCESS,
        current_state: ServiceState::Running,
        controls_accepted: ServiceControlAccept::STOP
            | ServiceControlAccept::SHUTDOWN
            | ServiceControlAccept::SESSION_CHANGE,
        exit_code: ServiceExitCode::Win32(0),
        checkpoint: 0,
        wait_hint: Duration::default(),
        process_id: None,
    })?;

    log_info!("🚀 Служба запущена и готова принимать события сессий");

    let db = Arc::new(AppDb::new(&config.paths.log_dir).map_err(|e| {
        log_error!("Критическая ошибка БД: {}", e);
        e
    })?);

    if db
        .get_state("device_uuid")
        .ok()
        .flatten()
        .unwrap_or_default()
        .is_empty()
    {
        let uuid = uuid::Uuid::new_v4().to_string();
        let _ = db.set_state("device_uuid", &uuid);
        log_info!("[device] сгенерирован UUID: {}", uuid);
    }

    users::initialize_users(&db);

    let (admin_cmd_tx, mut admin_cmd_rx) = mpsc::channel::<AdminCommand>(10);
    let (session_state_tx, session_state_rx) = watch::channel(SessionState::Active);

    // 🆕 ========================================================================
    // ПРОВЕРКА И СИНХРОНИЗАЦИЯ РЕАЛЬНОГО СОСТОЯНИЯ СЕССИИ ПРИ СТАРТЕ
    // ========================================================================
    // Синхронизация session_locked — только это остаётся в sync-части
    log_info!("🔍 [Startup] Проверка реального состояния сессии Windows через WTS API...");
    let _initial_session_state = sync_active_session_state(&db, &session_state_tx);
    //-------------------------------------------------------------

    // 3. Клонируем Arc для разных задач
    //    let config_for_api = Arc::clone(&config);
    let db_api = db.clone();
    let cmd_tx_for_api = admin_cmd_tx.clone();

    let config_for_discovery = Arc::clone(&config);
    let db_cloud = db.clone();
    let cmd_tx_for_cloud = admin_cmd_tx.clone();

    let db_for_discovery = db.clone();

    let db_track = db.clone();
    let mut session_state_rx_for_loop = session_state_rx.clone();

    // 🆕 Синхронизация задачи в планировщике
    {
        let enabled = db
            .get_setting("scheduler_enabled")
            .ok()
            .flatten()
            .map(|v| v == "1" || v == "true")
            .unwrap_or(true);
        let interval = db
            .get_setting("scheduler_interval_minutes")
            .ok()
            .flatten()
            .and_then(|v| v.parse().ok())
            .unwrap_or(5);
        scheduler::sync_watchdog_task(enabled, interval, &config.service.name);
    }

    let current_mode = get_current_mode(&db);
    log_info!("🎛️ Режим работы агента: {:?}", current_mode);

    let handle = std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
        // 🆕 РЕКОНСИЛЯЦИЯ ЛОКА ПРИ СТАРТЕ СЛУЖБЫ
        // Убиваем зависшие agent-cli/agent-tray, сбрасываем lock_status,
        // применяем политику заново.
        {
            let db_for_reconcile = db_track.clone();
            let _ = reconcile_lock_state_on_startup(&db_for_reconcile).await;
        }
        if current_mode.needs_local_api() {
            tokio::spawn(start_api_server(db_api, Arc::clone(&config), cmd_tx_for_api));
            tokio::spawn(start_discovery_listener(config_for_discovery, db_for_discovery.clone()));
        } else {
            log_info!("ℹ️ Локальный API отключён (режим Cloud)");
        }

        if current_mode.needs_cloud_client() {
            tokio::spawn(start_cloud_client(db_cloud, cmd_tx_for_cloud));
        } else {
            log_info!("ℹ️ Облачный клиент отключён (режим Local)");
        }

        // 🆕 Запуск трея через 2 секунды после старта API.
        //    Установщик сразу открывает веб-панель, поэтому API должен быть готов раньше трея.
        if current_mode.needs_local_api() {
            let port = config.service.api_port;
            tokio::spawn(async move {
                // Ждём, пока API ответит на /health (максимум 15 секунд)
                let client = reqwest::Client::builder()
                    .timeout(Duration::from_millis(500))
                    .build()
                    .ok();

                let api_url = format!("http://127.0.0.1:{}/health", port);
                let mut api_ready = false;

                if let Some(client) = client {
                    for attempt in 1..=15 {
                        match client.get(&api_url).send().await {
                            Ok(resp) if resp.status().is_success() => {
                                log_info!("[Tray] API готов (попытка {}), запускаю трей", attempt);
                                api_ready = true;
                                break;
                            }
                            _ => {
                                log_debug!("[Tray] API не отвечает (попытка {})", attempt);
                                tokio::time::sleep(Duration::from_secs(1)).await;
                            }
                        }
                    }
                }

                if !api_ready {
                    log_info!("[Tray] API не поднялся за 15 сек, всё равно запускаю трей");
                }

                if get_active_user_sid().is_some() {
                    log_info!("[Tray] запускаю трей");
                    std::thread::spawn(|| spawn_tray());
                } else {
                    log_debug!("[Tray] нет активного пользователя, трей не запускаю");
                }
            });
        }

        let mut tick_interval = tokio::time::interval(Duration::from_secs(config.service.tick_duration_secs));
        let mut tick_counter: u64 = 0;

        loop {
            tokio::select! {
                Some(cmd) = admin_cmd_rx.recv() => {
                    handle_admin_command(&db_track, cmd).await;
                }
                _ = session_state_rx_for_loop.changed() => {
                    handle_session_state_change(&db_track, &admin_cmd_tx, &session_state_rx_for_loop).await;
                }
                _ = tick_interval.tick() => {
                    tick_counter += 1;
                    handle_background_tick(&db_track, &session_state_rx_for_loop, tick_counter, &config).await;
                }
                Some(event) = session_event_rx.recv() => {
                    match event {
                        SessionEvent::Lock(sid) => {
                            log_info!("🔒 [SCM] Сессия {} заблокирована (Win+L)", sid);

                            if let Err(e) = db.set_state("session_locked", "true") {
                                log_error!("❌ [DB] session_locked=true: {}", e);
                            }
                            let _ = session_state_tx.send(SessionState::Locked);
                        }

                        SessionEvent::Unlock(sid) => {
                            log_info!("🔓 [SCM] Сессия {} разблокирована", sid);

                            if let Err(e) = db.set_state("session_locked", "false") {
                                log_error!("❌ [DB] session_locked=false: {}", e);
                            }
                            let _ = session_state_tx.send(SessionState::Active);

                            if let Some(user_sid) = get_active_user_sid() {
                                if let Some(state) = evaluate_access(&db, Some(&user_sid)) {
                                    let _ = apply_lock_state(&db, &user_sid, &state.policy).await;
                                }
                            }

                            // 🆕 При разблокировке — перезапуск трея (мьютекс защитит от дубля).
                            //    Нужно, если трей упал за время блокировки.
                            if get_active_user_sid().is_some() {
                                log_debug!("[Tray] сессия разблокирована — проверяю трей");
                                std::thread::spawn(|| spawn_tray());
                            }
                        }

                        SessionEvent::Stop => {
                            log_info!("🛑 Получен сигнал остановки от SCM");
                            break;
                        }
                    }
                }
            }
        }
    });
    });

    let _ = handle.join();
    log_info!("🛑 Цикл завершен, отправляем отчет в Windows...");

    // ФИКС: Перед выходом из функции ОБЯЗАТЕЛЬНО сообщаем Windows, что мы полностью остановились!
    // Для этого снова вызываем статус STOPPED.
    // Чтобы status_handle был доступен, убедитесь, что при его создании (строка 21-22 вашего кода)
    // переменная не была поглощена (не ушла внутрь замыкания).
    let _ = status_handle.set_service_status(ServiceStatus {
        service_type: ServiceType::OWN_PROCESS,
        current_state: ServiceState::Stopped, // МЫ ОСТАНОВИЛИСЬ ГРАЦИОЗНО
        controls_accepted: ServiceControlAccept::empty(), // Больше команды не принимаем
        exit_code: ServiceExitCode::Win32(0),
        checkpoint: 0,
        wait_hint: Duration::default(),
        process_id: None,
    });

    log_info!("🛑 Служба полностью остановлена");
    Ok(())
}

// ==========================================
// 3. БИЗНЕС-ЛОГИКА (ПРОЦЕДУРЫ)
// ==========================================

async fn handle_admin_command(db: &Arc<AppDb>, cmd: AdminCommand) {
    match cmd {
        AdminCommand::ForceLock => {
            log_info!("🔒 [Admin] Принудительная блокировка");
            if let Some(sid) = get_active_user_sid() {
                let _ = enforce_lock_procedure(db, &sid, "admin_force").await;
            } else {
                log_info!("ℹ️ [Admin] Активный пользователь не найден");
            }
        }

        AdminCommand::ForceUnlock => {
            log_info!("🔓 [Admin] Принудительная разблокировка");
            let _ = do_unlock_procedure(db).await;
        }

        AdminCommand::AutoMode => {
            log_info!("🔄 [Admin] Возврат в режим 'scheduled'");

            // 1. Глобальный режим
            let _ = db.set_state("control_mode", "scheduled");

            // 2. Сброс manual в daily_limits + применение политики
            if let Some(sid) = get_active_user_sid() {
                match db.return_to_scheduled(&sid) {
                    Ok(true) => log_info!("✅ [Admin] manual сброшен → scheduled"),
                    Ok(false) => log_info!("ℹ️ [Admin] manual не был активен"),
                    Err(e) => log_error!("❌ [Admin] return_to_scheduled: {}", e),
                }

                if let Some(state) = evaluate_access(db, Some(&sid)) {
                    let _ = apply_lock_state(db, &sid, &state.policy).await;
                }
            } else {
                log_info!("ℹ️ [Admin] Активный пользователь не найден");
            }
        }

        AdminCommand::SyncUsers => { /* ... */ }
    }
}

async fn handle_session_state_change(
    db: &Arc<AppDb>,
    admin_cmd_tx: &tokio::sync::mpsc::Sender<AdminCommand>,
    session_state_rx: &watch::Receiver<SessionState>,
) {
    let state = session_state_rx.borrow().clone();

    match state {
        SessionState::Locked => {
            log_info!("⏸️ Сессия заблокирована (Win+L). Счётчик остановлен.");
        }
        SessionState::Suspended => {
            log_info!("⏸️ Система ушла в сон/гибернацию. Счётчик остановлен.");
        }
        SessionState::Logoff => {
            log_info!("⏸️ Пользователь вышел из системы. Счётчик остановлен.");
        }
        SessionState::Active => {
            log_info!("▶️ Сессия активна. Контроль времени возобновлён.");

            let Some(sid) = get_active_user_sid() else {
                log_debug!("[Session] Active, но активного пользователя нет — пропуск sync");
                return;
            };

            log_info!("[Session] Синхронизация доступа для {}", sid);

            if let Err(e) = do_sync_user_access(db, admin_cmd_tx, &sid).await {
                log_error!("[Session] sync after Active failed: {:?}", e);
            }

            // 🆕 Запустить трей (мьютекс защитит от дубля)
            spawn_tray();
        }
    }
}

async fn handle_background_tick(
    db: &Arc<AppDb>,
    session_state_rx: &watch::Receiver<SessionState>,
    tick_counter: u64,
    _config: &Config,
) {
    let Some(sid) = get_active_user_sid() else {
        return;
    };
    let Some(state) = evaluate_access(db, Some(&sid)) else {
        return;
    };
    let policy = &state.policy;

    // Обновление mode manual → scheduled при истечении
    if matches!(policy.mode, ControlMode::Scheduled) {
        match db.return_to_scheduled(&sid) {
            Ok(true) => log_info!("⏰ [Tick] manual истёк — возврат в scheduled"),
            Ok(false) => {}
            Err(e) => log_error!("[Tick] return_to_scheduled: {:?}", e),
        }
    }

    // Применяем решение о блокировке
    let locked_now = match apply_lock_state(db, &sid, policy).await {
        Ok(v) => v,
        Err(e) => {
            log_error!("[Tick] apply_lock_state failed: {:?}", e);
            return; // или continue
        }
    };

    // Счётчик: только если сессия активна, не заблокировано, и контроль не выключен
    let session_active = is_session_active(session_state_rx);
    let should_credit =
        !matches!(policy.mode, ControlMode::Disabled) && session_active && !locked_now;

    if should_credit && tick_counter % 60 == 0 {
        process_screen_time_procedure(db, &sid).await;
    }
}

fn is_session_active(session_state_rx: &watch::Receiver<SessionState>) -> bool {
    *session_state_rx.borrow() == SessionState::Active
}

async fn process_screen_time_procedure(db: &Arc<AppDb>, sid: &str) {
    // 1. Считаем минуту
    if let Err(e) = db.add_screen_time(sid, 60) {
        log_error!("Ошибка записи времени: {}", e);
        return;
    }

    // 🆕 интернет-время
    let internet_paused = db.is_internet_paused(sid).unwrap_or(false);
    if internet_paused {
        return;
    }

    let today_limit = match db.ensure_today_limit(sid) {
        Ok(l) => l,
        Err(_) => return,
    };
    let effective_internet_limit =
        today_limit.internet_limit_sec + today_limit.extra_internet_time_min * 60;

    let (_, internet_time) = db.get_today_usage(sid).unwrap_or((0, 0));

    // Не считаем, если лимит уже исчерпан (или 0 = ноль секунд — но это не «безлимит»)
    // Если у тебя -1 = безлимит, то:
    if effective_internet_limit >= 0 && (internet_time as i32) >= effective_internet_limit {
        return; // лимит уже превышен, не тикаем
    }

    if let Err(e) = db.add_internet_time(sid, 60) {
        log_error!("Ошибка записи интернет-времени: {}", e);
    }

    let today_limit = match db.ensure_today_limit(sid) {
        Ok(limit) => limit,
        Err(_) => return,
    };

    let (today_time, _) = match db.get_today_usage(sid) {
        Ok(val) => val,
        Err(e) => {
            log_error!("Ошибка чтения времени: {}", e);
            return;
        }
    };

    // 2. Эффективный лимит с extra_time
    let effective_limit_sec = today_limit.limit_sec + today_limit.extra_time_min * 60;

    // 3. Warning
    let warning_mins: u32 = db
        .get_setting("warning_minutes")
        .ok()
        .flatten()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);

    if warning_mins > 0 && effective_limit_sec > 0 {
        let remaining_sec = (effective_limit_sec as u32).saturating_sub(today_time);
        let remaining_min = remaining_sec / 60;
        if remaining_min <= warning_mins && remaining_min > 0 {
            log_info!(
                "⚠️ Предупреждение: осталось {} мин до лимита",
                remaining_min
            );
        }
    }

    // 🆕 БЛОКИРОВКА УБРАНА — этим занимается apply_lock_state
}

async fn enforce_lock_procedure(db: &Arc<AppDb>, sid: &str, reason: &str) -> bool {
    let config = Config::new();
    let exe_path = match config.get_cli_path() {
        Some(path) => path, // PathBuf
        None => {
            log_error!("❌ КРИТИЧЕСКАЯ ОШИБКА: agent-cli.exe не найден ни в одном из путей");
            let _ = db.set_state("lock_status", "true");
            return false;
        }
    };

    log_info!("🧹 [Lock] Очистка зависших процессов agent-cli.exe...");
    let _ = std::process::Command::new("taskkill")
        .args(&["/F", "/IM", "agent-cli.exe", "/T"])
        .output();

    tokio::time::sleep(std::time::Duration::from_millis(300)).await;

    log_info!(
        "🔍 Выполняю блокировку: {} (причина: {})",
        exe_path.display(),
        reason
    );

    let delay_sec: u32 = db
        .get_setting("lock_delay_sec")
        .ok()
        .flatten()
        .and_then(|s| s.parse().ok())
        .unwrap_or(5);

    let args = format!("lock {}", delay_sec);

    // execute_in_user_session ожидает &str — конвертируем PathBuf
    let exe_str = match exe_path.to_str() {
        Some(s) => s,
        None => {
            log_error!(
                "❌ [Lock] Путь к agent-cli.exe не является валидным UTF-8: {:?}",
                exe_path
            );
            let _ = db.set_state("lock_status", "true");
            return false;
        }
    };

    match agent_windows::session::execute_in_user_session(exe_str, &args) {
        Ok(_) => {
            log_info!("✅ Команда блокировки успешно отправлена в сессию пользователя");
            let _ = db.set_state("lock_status", "true");
            let _ = db.log_lock_event(sid, reason);
            true
        }
        Err(e) => {
            log_error!("❌ Ошибка выполнения блокировки: {}", e);
            let _ = db.set_state("lock_status", "true");
            false
        }
    }
}

async fn do_unlock_procedure(db: &Arc<AppDb>) {
    let _ = db.set_state("lock_status", "false");

    let _ = std::process::Command::new("taskkill")
        .args(&["/F", "/IM", "agent-cli.exe", "/T"])
        .output();
}

/// Приводит lock_status в соответствие с политикой.
/// Идемпотентна: если состояние совпадает — ничего не делает.
/// Возвращает: `true`, если система сейчас должна быть заблокирована.
async fn apply_lock_state(
    db: &Arc<AppDb>,
    sid: &str,
    policy: &PolicyCheckResult,
) -> Result<bool, ()> {
    log_debug!("apply_lock_state");

    let is_locked = db.get_state("lock_status").map_err(|_| ())?.as_deref() == Some("true");
    let want_lock = agent_windows::policy::should_lock(policy);

    log_debug!("is_locked, wantlocked: {}, {}", is_locked, want_lock);

    // ===== Блокировка экрана =====
    let screen_locked = if want_lock && !is_locked {
        log_info!("Надо лочить, а не залочено!");
        enforce_lock_procedure(db, sid, "policy").await;
        true
    } else if !want_lock && is_locked {
        db.set_state("lock_status", "false").map_err(|_| ())?;
        do_unlock_procedure(db).await;
        false
    } else {
        is_locked
    };

    if let ControlMode::Manual {
        until: _,
        action: _,
    } = policy.mode
    {
        // Всегда разблокировать интернет при переходе в ручной режим
        if let Some(sid) = get_active_user_sid() {
            if let Err(e) = agent_windows::firewall::set_block_enabled(&sid, false) {
                log_error!("[apply_lock_state] Failed to unblock internet: {}", e);
            }
            let _ = db.set_internet_paused(&sid, false, "manual");
        }

        return Ok(screen_locked);
    }

    // ===== Блокировка интернета по лимиту =====
    let is_internet_paused = db.is_internet_paused(sid).unwrap_or(false);
    let source = db.get_internet_paused_source(sid).unwrap_or_default();

    if policy.internet_exceeded && !is_internet_paused {
        log_info!(
            "🌐 [Lock] Превышен лимит интернета ({}/{} сек) — блокирую",
            policy.internet_time_sec,
            policy.internet_limit_sec
        );

        match agent_windows::firewall::ensure_block_rule(sid) {
            Ok(_) => match agent_windows::firewall::set_block_enabled(sid, true) {
                Ok(_) => {
                    let _ = db.set_internet_paused(sid, true, "limit");
                    let _ = db.set_state("internet_blocked", "true");
                    log_info!("✅ [Lock] Интернет заблокирован (по лимиту)");
                }
                Err(e) => log_error!("❌ [Lock] set_block_enabled(true): {}", e),
            },
            Err(e) => log_error!("❌ [Lock] ensure_block_rule: {}", e),
        }
    } else if !policy.internet_exceeded && is_internet_paused && source == "limit" {
        log_info!("🌐 [Lock] Лимит интернета в норме — снимаю блокировку");

        if let Err(e) = agent_windows::firewall::set_block_enabled(sid, false) {
            log_error!("❌ [Lock] set_block_enabled(false): {}", e);
        }
        let _ = db.set_internet_paused(sid, false, "");
        let _ = db.set_state("internet_blocked", "false");
    }

    // ===== 🆕 Блокировка интернета при блокировке экрана =====
    let block_internet_on_screen = db
        .get_setting("lock_internet_on_screen_lock")
        .ok()
        .flatten()
        .map(|v| v == "1" || v == "true")
        .unwrap_or(false);

    // Перечитываем состояние: первый блок мог его изменить
    let is_internet_paused = db.is_internet_paused(sid).unwrap_or(false);
    let source = db.get_internet_paused_source(sid).unwrap_or_default();

    if screen_locked && block_internet_on_screen && !is_internet_paused {
        log_info!("🌐 [Lock] Экран заблокирован — блокирую интернет");
        match agent_windows::firewall::ensure_block_rule(sid) {
            Ok(_) => match agent_windows::firewall::set_block_enabled(sid, true) {
                Ok(_) => {
                    let _ = db.set_internet_paused(sid, true, "screen_lock");
                    let _ = db.set_state("internet_blocked", "true");
                }
                Err(e) => log_error!("❌ [Lock] screen_lock set_block_enabled(true): {}", e),
            },
            Err(e) => log_error!("❌ [Lock] screen_lock ensure_block_rule: {}", e),
        }
    } else if !screen_locked && is_internet_paused && source == "screen_lock" {
        log_info!("🌐 [Lock] Экран разблокирован — снимаю блокировку интернета (screen_lock)");
        if let Err(e) = agent_windows::firewall::set_block_enabled(sid, false) {
            log_error!("❌ [Lock] screen_lock set_block_enabled(false): {}", e);
        }
        let _ = db.set_internet_paused(sid, false, "");
        let _ = db.set_state("internet_blocked", "false");
    }

    Ok(screen_locked)
}

// ==========================================
// 4. УТИЛИТЫ ЛОГИРОВАНИЯ
// ==========================================

fn log_to_file(message: &str) {
    use std::io::Write;
    let config = Config::new();
    let _ = std::fs::create_dir_all(&config.paths.log_dir);
    let log_path = std::path::Path::new(&config.paths.log_dir).join("service.log");

    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path)
    {
        let timestamp = Local::now().format("[%Y-%m-%d %H:%M:%S]");
        let _ = writeln!(file, "{} {}", timestamp, message);
    }
    println!("{}", message);
}

// ТОЧКА ВХОДА MAIN
fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 0. Panic hook — ПЕРВЫМ делом, до всего остального
    std::panic::set_hook(Box::new(|info| {
        let msg = if let Some(s) = info.payload().downcast_ref::<&str>() {
            s.to_string()
        } else if let Some(s) = info.payload().downcast_ref::<String>() {
            s.clone()
        } else {
            "unknown panic".to_string()
        };

        let location = info
            .location()
            .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
            .unwrap_or_else(|| "unknown".to_string());

        let log_dir = r"C:\ProgramData\ParentalControl\logs";
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();
        let dir = format!("{}\\{}", log_dir, today);
        let _ = std::fs::create_dir_all(&dir);
        let log_path = format!("{}\\panic.log", dir);

        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
        {
            use std::io::Write;
            let _ = writeln!(
                file,
                "[{}] PANIC at {}: {}",
                chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
                location,
                msg
            );
        }
    }));

    // 1. Принудительно фиксируем рабочую директорию программы
    if let Ok(exe_path) = env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            let _ = env::set_current_dir(exe_dir);
        }
    }

    // 2. Разбираем аргументы командной строки
    let args: Vec<String> = env::args().collect();
    if args.len() > 1 {
        if args[1] == "install" {
            install_service()?;
            println!("✅ Служба успешно зарегистрирована в системе.");
            return Ok(());
        } else if args[1] == "uninstall" {
            uninstall_service()?;
            println!("✅ Служба успешно удалена из системы.");
            return Ok(());
        }
    }

    // 3. Запуск диспетчера служб Windows (если аргументов нет)
    let config = Config::load().map_err(|e| {
        log_to_file(&format!(
            "❌ Критическая ошибка: не удалось загрузить config.toml при старте диспетчера: {}",
            e
        ));
        e
    })?;

    service_dispatcher::start(config.service.name, ffi_service_main)?;
    Ok(())
}

// Полная инициализация ServiceInfo со всеми обязательными полями в формате OsString
fn install_service() -> Result<(), Box<dyn std::error::Error>> {
    let config = Config::load()?;
    let exe_path = env::current_exe()?;

    let manager = ServiceManager::local_computer(
        Option::<&str>::None,
        ServiceManagerAccess::CONNECT | ServiceManagerAccess::CREATE_SERVICE,
    )?;

    // Заполняем абсолютно все поля, преобразуя строки через .into() в OsString
    let service_info = ServiceInfo {
        name: config.service.name.clone().into(),
        display_name: config.service.name.clone().into(),
        executable_path: exe_path,
        launch_arguments: vec![],
        dependencies: vec![],
        service_type: ServiceType::OWN_PROCESS,
        start_type: ServiceStartType::AutoStart, // CamelCase для 0.8.1
        error_control: ServiceErrorControl::Normal, // CamelCase для 0.8.1
        account_name: None,
        account_password: None,
    };

    // Создаем службу в системе
    let _service = manager.create_service(
        &service_info,
        ServiceAccess::START | ServiceAccess::QUERY_STATUS,
    )?;

    Ok(())
}

fn uninstall_service() -> Result<(), Box<dyn std::error::Error>> {
    let config = Config::load()?;
    let manager =
        ServiceManager::local_computer(Option::<&str>::None, ServiceManagerAccess::CONNECT)?;

    let service = manager.open_service(&config.service.name, ServiceAccess::DELETE)?;
    service.delete()?;
    Ok(())
}

/// Проверяет реальное состояние сессии Windows через WTS API.
/// Возвращает `true`, если сессия заблокирована.
fn check_session_state_via_wts() -> bool {
    unsafe {
        let session_id = WTSGetActiveConsoleSessionId();
        if session_id == 0xFFFFFFFF {
            log_debug!(" [WTS] Нет активной консольной сессии");
            return false;
        }

        let mut buffer: *mut u8 = std::ptr::null_mut();
        let mut bytes_returned: u32 = 0;

        let result = WTSQuerySessionInformationW(
            None,
            session_id,
            WTS_INFO_CLASS(25), // WTSSessionInfoEx
            &mut buffer as *mut _ as *mut _,
            &mut bytes_returned,
        );

        if result.is_ok() && !buffer.is_null() && bytes_returned >= 16 {
            // Читаем SessionFlags (смещение 12 байт)
            let session_flags = u32::from_ne_bytes([
                *buffer.add(12),
                *buffer.add(13),
                *buffer.add(14),
                *buffer.add(15),
            ]);

            let is_locked = (session_flags & 0x00000001) != 0;

            // ✅ ПРАВИЛЬНО: передаем литерал и аргументы напрямую в макрос
            log_debug!(
                " [WTS] SessionFlags: 0x{:08X}, is_locked: {}",
                session_flags,
                is_locked
            );

            WTSFreeMemory(buffer as *mut _);
            is_locked
        } else {
            log_debug!(" [WTS] Ошибка получения информации о сессии");
            if !buffer.is_null() {
                WTSFreeMemory(buffer as *mut _);
            }
            false
        }
    }
}

/// Синхронизирует состояние сессии Windows с БД и отправляет в watch-канал.
/// Возвращает итоговое состояние.
fn sync_active_session_state(
    db: &Arc<AppDb>,
    session_state_tx: &watch::Sender<SessionState>,
) -> SessionState {
    let is_session_locked = check_session_state_via_wts();
    let db_locked = db
        .get_state("session_locked")
        .ok()
        .flatten()
        .map(|s| s == "true")
        .unwrap_or(false);

    if is_session_locked != db_locked {
        log_info!(
            "🔄 [Startup] Рассинхрон session_locked: DB={}, WTS={}. Исправляю.",
            db_locked,
            is_session_locked
        );
        let _ = db.set_state(
            "session_locked",
            if is_session_locked { "true" } else { "false" },
        );
    }

    let initial_state = if is_session_locked {
        SessionState::Locked
    } else {
        SessionState::Active
    };

    let _ = session_state_tx.send(initial_state.clone());

    initial_state
}

fn spawn_tray() {
    let exe_path = match std::env::current_exe() {
        Ok(p) => p.parent().map(|d| d.join("agent-tray.exe")),
        Err(e) => {
            log_error!("[Tray] current_exe failed: {:?}", e);
            return;
        }
    };

    let Some(exe_path) = exe_path else {
        log_error!("[Tray] не могу определить каталог exe");
        return;
    };

    if !exe_path.exists() {
        log_error!("[Tray] agent-tray.exe не найден: {:?}", exe_path);
        return;
    }

    log_debug!("[Tray] спавн {}", exe_path.display());

    if let Err(e) =
        agent_windows::session::execute_in_user_session(exe_path.to_str().unwrap_or(""), "")
    {
        log_error!("[Tray] spawn failed: {:?}", e);
    }
}

/// Убивает все зависшие процессы agent-cli.exe и agent-tray.exe.
/// Вызывается при старте службы, чтобы не осталось «призрачных» блокировок,
/// которые уже не управляются (например, после перезагрузки).
fn cleanup_stale_agents() {
    log_info!("🧹 [Startup] Очистка зависших процессов agent-cli.exe / agent-tray.exe...");

    for image in ["agent-cli.exe", "agent-tray.exe"] {
        match std::process::Command::new("taskkill")
            .args(&["/F", "/IM", image, "/T"])
            .output()
        {
            Ok(out) => {
                let stdout = String::from_utf8_lossy(&out.stdout);
                let stderr = String::from_utf8_lossy(&out.stderr);
                log_debug!(
                    "[Startup] taskkill {} → status={:?}, out={}, err={}",
                    image,
                    out.status.code(),
                    stdout.trim(),
                    stderr.trim()
                );
            }
            Err(e) => log_error!("[Startup] taskkill {} failed: {}", image, e),
        }
    }
}

/// Приводит реальное состояние блокировки в соответствие с политикой.
///
/// Логика:
///   1. Убить зависшие agent-cli.exe / agent-tray.exe (могли остаться после перезагрузки).
///   2. Сбросить lock_status = false (реального процесса больше нет).
///   3. Снять «залипшие» флаги интернет-блокировки, если источник = screen_lock
///      (т.к. screen_lock больше не актуален — окна нет).
///   4. Вызвать apply_lock_state: он либо залочит заново (если политика требует),
///      либо оставит разблокированным.
///
/// Возвращает `true`, если по итогу система должна быть заблокирована.
async fn reconcile_lock_state_on_startup(db: &Arc<AppDb>) -> bool {
    log_info!("🔍 [Startup] Реконсиляция состояния блокировки...");

    // Шаг 1. Убиваем всё, что могло остаться.
    cleanup_stale_agents();

    // Небольшая пауза, чтобы Windows успела прибить процессы и отпустить мьютексы.
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Шаг 2. Реального процесса agent-cli больше нет → лок снят.
    let db_locked = db.get_state("lock_status").ok().flatten().as_deref() == Some("true");

    if db_locked {
        log_info!(
            "♻️ [Startup] БД считала lock_status=true, но agent-cli убит — сбрасываю в false"
        );
        let _ = db.set_state("lock_status", "false");
    }

    // Шаг 3. Если интернет был заблокирован «из-за лока экрана» — снимаем,
    //         т.к. лока экрана больше нет.
    if let Some(sid) = get_active_user_sid() {
        let source = db.get_internet_paused_source(&sid).unwrap_or_default();
        let paused = db.is_internet_paused(&sid).unwrap_or(false);

        if paused && source == "screen_lock" {
            log_info!(
                "🌐 [Startup] Интернет был заблокирован из-за лока экрана — снимаю блокировку"
            );
            if let Err(e) = agent_windows::firewall::set_block_enabled(&sid, false) {
                log_error!("❌ [Startup] set_block_enabled(false): {}", e);
            }
            let _ = db.set_internet_paused(&sid, false, "");
            let _ = db.set_state("internet_blocked", "false");
        }

        // Шаг 4. Применяем политику к реальности.
        if let Some(state) = evaluate_access(db, Some(&sid)) {
            match apply_lock_state(db, &sid, &state.policy).await {
                Ok(locked) => {
                    log_info!("✅ [Startup] Реконсиляция завершена: locked={}", locked);
                    return locked;
                }
                Err(_) => {
                    log_error!("❌ [Startup] apply_lock_state вернул ошибку");
                }
            }
        } else {
            log_debug!("[Startup] evaluate_access вернул None — нечего применять");
        }
    } else {
        log_info!("ℹ️ [Startup] Активного пользователя нет — только очистка выполнена");
    }

    false
}
