// src/bin/tray.rs
#![windows_subsystem = "windows"]
use std::{
    path::Path,
    sync::{Arc, Mutex, OnceLock},
    time::{Duration, Instant},
};

use agent_windows::{config::Config, log_debug, log_error, log_info};
use image::GenericImageView;
use tao::{
    event::{Event, StartCause},
    event_loop::{ControlFlow, EventLoopBuilder},
};
use tray_icon::{
    menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem},
    TrayIconBuilder, TrayIconEvent,
};

// ===== Константы =====

/// Как часто event loop просыпается, чтобы вычитать каналы tray-icon
/// (клик по иконке, выбор в меню). Не имеет отношения к свежести данных —
/// за это отвечает tray.update_interval_sec из config.toml.
/// 200 мс — компромисс между отзывчивостью и CPU.
const LOOP_TICK_MS: u64 = 200;

/// 🆕 Как часто обновлять рантайм-настройки (lock_warning_sec) в фоне.
const SETTINGS_REFRESH_SEC: u64 = 180;

static LAST_STATUS: OnceLock<Mutex<Option<Status>>> = OnceLock::new();

/// 🆕 Идентификатор события, для которого уже показывали авто-popup.
/// Хранит строку вида "kind:event_at_minute" — чтобы не спамить
/// при каждом обновлении статуса, но показать заново для нового события.
static LAST_WARNED_EVENT: OnceLock<Mutex<Option<String>>> = OnceLock::new();

// ===== Status =====

#[derive(Debug, Clone, Default)]
struct Status {
    current_user: String,
    screen_time_sec: u32,
    screen_limit_sec: u32,
    is_locked: bool,
    mode: String,
    internet_paused: bool,
    internet_time_sec: u32,
    internet_limit_sec: u32,
    time_to_event: Option<TimeToEvent>,
}

#[derive(Debug, Clone)]
struct TimeToEvent {
    kind: String,
    minutes: u32,
    #[allow(dead_code)]
    reason: String,
}

struct TrayEnv {
    api_base: String,
    token_file: String,
    update_interval: Duration,
    /// 🆕 Кэш lock_warning_sec из /api/settings.
    /// 0 = не показывать автоматически.
    /// Заполняется в main() при старте и периодически обновляется
    /// в фоновом потоке статуса.
    lock_warning_sec: Arc<Mutex<u32>>,
}

impl TrayEnv {
    fn from_config(cfg: &Config) -> Self {
        Self {
            api_base: format!("http://{}:{}", cfg.tray.host, cfg.service.api_port),
            token_file: Path::new(&cfg.paths.data_dir)
                .join("tray_token.txt")
                .to_string_lossy()
                .to_string(),
            update_interval: Duration::from_secs(cfg.tray.update_interval_sec.max(1)),
            lock_warning_sec: Arc::new(Mutex::new(0)),
        }
    }

    fn get_lock_warning_sec(&self) -> u32 {
        self.lock_warning_sec.lock().map(|g| *g).unwrap_or(0)
    }
}

// ===== Токен =====

fn read_token(path: &str) -> Option<String> {
    std::fs::read_to_string(path)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

// ===== HTTP =====

fn fetch_status(client: &reqwest::blocking::Client, api_base: &str, token: &str) -> Option<Status> {
    let res = client
        .get(format!("{}/api/status", api_base))
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .ok()?;

    if !res.status().is_success() {
        return None;
    }

    let json: serde_json::Value = res.json().ok()?;

    let time_to_event = json.get("time_to_event").and_then(|v| {
        if v.is_null() {
            return None;
        }
        Some(TimeToEvent {
            kind: v.get("kind")?.as_str()?.to_string(),
            minutes: v.get("minutes")?.as_u64()? as u32,
            reason: v
                .get("reason")
                .and_then(|r| r.as_str())
                .unwrap_or("")
                .to_string(),
        })
    });

    Some(Status {
        current_user: json
            .get("current_user")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        screen_time_sec: json
            .get("screen_time_sec")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u32,
        screen_limit_sec: json
            .get("daily_limit_sec")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u32,
        is_locked: json
            .get("is_locked")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        mode: json
            .get("mode")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        internet_paused: json
            .get("internet_paused")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        internet_time_sec: json
            .get("internet_time_sec")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u32,
        internet_limit_sec: json
            .get("internet_limit_sec")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u32,
        time_to_event,
    })
}

/// 🆕 Загружает рантайм-настройки из /api/settings.
/// Ответ — массив `[{ key, value, is_secret, ... }]`.
/// Возвращает None при ошибке сети/авторизации или если поля нет.
fn fetch_runtime_settings(
    client: &reqwest::blocking::Client,
    api_base: &str,
    token: &str,
) -> Option<u32> {
    let res = client
        .get(format!("{}/api/settings", api_base))
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .ok()?;

    if !res.status().is_success() {
        return None;
    }

    let items: Vec<serde_json::Value> = res.json().ok()?;

    items
        .iter()
        .find(|item| item.get("key").and_then(|k| k.as_str()) == Some("lock_warning_sec"))
        .and_then(|item| item.get("value"))
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse::<u32>().ok())
}

// ===== Иконки =====

fn try_load(path: &str) -> Result<tray_icon::Icon, ()> {
    let img = image::open(path).map_err(|_| ())?;
    let (w, h) = img.dimensions();
    tray_icon::Icon::from_rgba(img.into_rgba8().into_raw(), w, h).map_err(|_| ())
}

fn make_circle_icon(size: u32, r: u8, g: u8, b: u8) -> tray_icon::Icon {
    let mut data = Vec::with_capacity((size * size * 4) as usize);
    let cx = size as i32 / 2;
    let cy = size as i32 / 2;
    let radius2 = (cx - 2) * (cx - 2);
    for y in 0..size as i32 {
        for x in 0..size as i32 {
            let dx = x - cx;
            let dy = y - cy;
            if dx * dx + dy * dy <= radius2 {
                data.extend_from_slice(&[r, g, b, 255]);
            } else {
                data.extend_from_slice(&[0, 0, 0, 0]);
            }
        }
    }
    tray_icon::Icon::from_rgba(data, size, size).expect("icon")
}

fn load_icon(path: &str) -> tray_icon::Icon {
    if let Ok(icon) = try_load(path) {
        log_info!("[icon] загружен: {}", path);
        return icon;
    }

    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            let dirs = [
                exe_dir.to_path_buf(),
                exe_dir.parent().unwrap_or(exe_dir).to_path_buf(),
            ];
            for dir in dirs {
                if let Ok(icon) = try_load(&dir.join(path).to_string_lossy()) {
                    log_info!("[icon] загружен от exe: {}", path);
                    return icon;
                }
            }
        }
    }

    log_info!("[icon] fallback круг: {}", path);
    let (r, g, b) = match path {
        p if p.contains("green") => (34u8, 197u8, 94u8),
        p if p.contains("yellow") => (234u8, 179u8, 8u8),
        p if p.contains("red") => (239u8, 68u8, 68u8),
        p if p.contains("gray") || p.contains("grey") => (156u8, 163u8, 175u8),
        _ => (128u8, 128u8, 128u8),
    };
    make_circle_icon(32, r, g, b)
}

// ===== Форматирование =====

fn format_secs(sec: u32) -> String {
    let m = sec / 60;
    let h = m / 60;
    let rem = m % 60;
    if h > 0 {
        format!("{}ч {}м", h, rem)
    } else {
        format!("{}м", m)
    }
}

fn build_tooltip(status: &Status) -> String {
    let mut lines = vec!["🛡️ Родительский контроль".to_string()];

    if status.current_user.is_empty() {
        lines.push("Нет активных сессий".to_string());
    } else {
        lines.push(format!("👤 {}", status.current_user));

        if status.screen_limit_sec > 0 {
            let screen_remaining = status
                .screen_limit_sec
                .saturating_sub(status.screen_time_sec);
            lines.push(format!(
                "📺 Экран: {} осталось",
                format_secs(screen_remaining)
            ));
        }

        if status.internet_paused {
            lines.push("🌐 Интернет: ⏸ на паузе".to_string());
        }
    }

    lines.join("\n")
}

// ===== События =====

#[derive(Debug)]
enum UserEvent {
    UpdateStatus(Status),
}

// ===== Авто-показ popup =====

/// 🆕 Стабильный идентификатор события. Округляем время наступления до минуты,
/// чтобы при дрожании `minutes` между опросами не считать событие «новым».
fn event_id(tt: &TimeToEvent) -> String {
    let now_sec = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let event_at_min = (now_sec + tt.minutes as u64 * 60) / 60;
    format!("{}:{}", tt.kind, event_at_min)
}

/// 🆕 Проверяет, не пора ли автоматически показать popup, и показывает его.
/// Возвращает true, если popup был показан в этот раз.
fn maybe_show_warning_popup(
    status: &Status,
    client: &Arc<reqwest::blocking::Client>,
    env: &TrayEnv,
) -> bool {
    let Some(tt) = &status.time_to_event else {
        return false;
    };

    // 🆕 порог берём из кэша настроек (обновляется в фоне)
    let warn_sec = env.get_lock_warning_sec();
    if warn_sec == 0 {
        return false;
    }

    // time_to_event.minutes — в минутах. Переводим в секунды для сравнения.
    let remaining_sec = tt.minutes as u64 * 60;
    if remaining_sec > warn_sec as u64 {
        return false;
    }

    // Панель уже видна — не трогаем
    if agent_windows::tray_ui::is_visible() {
        return false;
    }

    let id = event_id(tt);
    let warned = LAST_WARNED_EVENT.get_or_init(|| Mutex::new(None));
    let already = {
        let guard = match warned.lock() {
            Ok(g) => g,
            Err(_) => return false,
        };
        guard.as_deref() == Some(id.as_str())
    };
    if already {
        return false;
    }

    log_info!(
        "[auto-popup] событие '{}' через {} мин (порог {} сек) — показываю панель",
        tt.kind,
        tt.minutes,
        warn_sec
    );

    show_popup(client, &env.api_base, &env.token_file);

    if let Ok(mut guard) = warned.lock() {
        *guard = Some(id);
    }
    true
}

// ===== show_popup =====

fn show_popup(client: &Arc<reqwest::blocking::Client>, api_base: &str, token_file: &str) {
    log_info!("[popup] show_popup вызван");

    let Some(status) = get_last_status() else {
        log_info!("[popup] нет данных статуса — не показываю");
        return;
    };

    let data = agent_windows::tray_ui::PopupData {
        username: status.current_user.clone(),
        screen_remaining_sec: status
            .screen_limit_sec
            .saturating_sub(status.screen_time_sec),
        screen_limit_sec: status.screen_limit_sec,
        internet_paused: status.internet_paused,
        internet_remaining_sec: status
            .internet_limit_sec
            .saturating_sub(status.internet_time_sec),
        internet_limit_sec: status.internet_limit_sec,
        mode: status.mode.clone(),
        is_locked: status.is_locked,
        time_to_event_minutes: status.time_to_event.as_ref().map(|tt| tt.minutes),
    };

    log_info!(
        "[popup] показываю: user={}, screen={}/{}s, paused={}",
        data.username,
        data.screen_remaining_sec,
        data.screen_limit_sec,
        data.internet_paused
    );

    // owned, чтобы замыкание 'static
    let api_base = api_base.to_string();
    let token_file = token_file.to_string();
    let client_cb = client.clone();

    agent_windows::tray_ui::show(
        data,
        move |should_pause| {
            log_debug!("[cb] toggle pause → {}", should_pause);
            if let Some(token) = read_token(&token_file) {
                let url = if should_pause {
                    format!("{}/api/internet/pause", api_base)
                } else {
                    format!("{}/api/internet/resume", api_base)
                };
                match client_cb
                    .post(url)
                    .header("Authorization", format!("Bearer {}", token))
                    .send()
                {
                    Ok(resp) if resp.status().is_success() => {
                        log_info!("[cb] pause toggle OK ({})", resp.status());
                    }
                    Ok(resp) => {
                        log_error!(
                            "[cb] pause toggle failed: HTTP {} (body: {:?})",
                            resp.status(),
                            resp.text().unwrap_or_default()
                        );
                    }
                    Err(e) => {
                        log_error!("[cb] pause toggle error: {:?}", e);
                    }
                }
            } else {
                log_error!("[cb] pause toggle: токен не найден ({})", token_file);
            }
        },
        || {
            log_debug!("[cb] окно закрыто");
        },
    );

    log_debug!("[popup] tray_ui::show вернулся");
}

fn get_last_status() -> Option<Status> {
    LAST_STATUS
        .get()
        .and_then(|m| m.lock().ok())
        .and_then(|guard| guard.clone())
}

// ===== main =====

fn main() {
    // 1. Config — до логгера, чтобы взять log_dir
    let config = Config::new();

    // 2. Логгер
    let logs_path = std::path::Path::new(&config.paths.log_dir);
    agent_windows::logger::init_global_logger(
        &logs_path.to_string_lossy(),
        "agent_tray",
        agent_windows::logger::LogLevel::Info,
        14,
    );

    // 3. Мьютекс — защита от второго экземпляра
    let _mutex_guard = {
        use windows::{
            core::w,
            Win32::{
                Foundation::{GetLastError, ERROR_ALREADY_EXISTS},
                System::Threading::CreateMutexW,
            },
        };

        unsafe {
            let handle = CreateMutexW(None, true, w!("Global\\AgentTrayMutex"));
            match handle {
                Ok(h) => {
                    if GetLastError() == ERROR_ALREADY_EXISTS {
                        std::process::exit(0);
                    }
                    h
                }
                Err(_) => {
                    std::process::exit(1);
                }
            }
        }
    };

    // 4. TrayEnv — из config
    let env = Arc::new(TrayEnv::from_config(&config));

    log_debug!("=== agent-tray стартовал ===");
    log_debug!(
        "[main] api_base={}, token_file={}, update_interval={}s",
        env.api_base,
        env.token_file,
        env.update_interval.as_secs()
    );

    let tray_icon_image = load_icon("static/icons/tray-shield.png");

    let client = Arc::new(
        reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(3))
            .build()
            .expect("http client"),
    );
    log_debug!("[main] http client создан");

    // 🆕 Первичная загрузка рантайм-настроек (один раз, синхронно).
    //    Если API ещё не поднялся — останется 0, а фоновый поток
    //    догрузит при следующем тике.
    if let Some(token) = read_token(&env.token_file) {
        match fetch_runtime_settings(&client, &env.api_base, &token) {
            Some(v) => {
                if let Ok(mut g) = env.lock_warning_sec.lock() {
                    *g = v;
                }
                log_info!("[settings] lock_warning_sec = {}", v);
            }
            None => {
                log_error!("[settings] не удалось загрузить lock_warning_sec при старте");
            }
        }
    } else {
        log_error!("[settings] токен не найден, lock_warning_sec останется 0");
    }

    let event_loop = EventLoopBuilder::<UserEvent>::with_user_event().build();
    let proxy = event_loop.create_proxy();

    log_debug!("[main] event_loop создан");

    LAST_STATUS.set(Mutex::new(None)).ok();
    LAST_WARNED_EVENT.set(Mutex::new(None)).ok();
    log_debug!("[main] LAST_STATUS / LAST_WARNED_EVENT инициализированы");

    let tray_menu = Menu::new();
    let item_open = MenuItem::new("🌐 Открыть панель", true, None);
    let item_pause = MenuItem::new("⏸ Пауза интернета", true, None);
    let item_quit = MenuItem::new("Выход", true, None);
    let _ = tray_menu.append_items(&[
        &item_open,
        &PredefinedMenuItem::separator(),
        &item_pause,
        &PredefinedMenuItem::separator(),
        &item_quit,
    ]);
    log_debug!("[main] меню собрано");

    let tray_icon = Arc::new(Mutex::new(
        TrayIconBuilder::new()
            .with_menu(Box::new(tray_menu))
            .with_menu_on_left_click(false)
            .with_tooltip("Родительский контроль")
            .with_icon(tray_icon_image)
            .build()
            .expect("tray icon"),
    ));

    log_debug!("[main] иконка в трее установлена");

    let menu_channel = MenuEvent::receiver().clone();
    let tray_channel = TrayIconEvent::receiver().clone();

    let item_open_id = item_open.id().clone();
    let item_pause_id = item_pause.id().clone();
    let item_quit_id = item_quit.id().clone();

    // Фоновый поток — читает статус раз в N сек, а настройки — раз в SETTINGS_REFRESH_SEC
    {
        let proxy = proxy.clone();
        let client = client.clone();
        let env_bg = env.clone();
        std::thread::spawn(move || {
            log_debug!("[bg] поток статуса стартовал");
            let mut tick: u64 = 0;
            let settings_refresh_every =
                (SETTINGS_REFRESH_SEC / env_bg.update_interval.as_secs().max(1)).max(1);

            loop {
                if let Some(token) = read_token(&env_bg.token_file) {
                    // 🆕 периодический refresh настроек
                    if tick % settings_refresh_every == 0 {
                        if let Some(v) = fetch_runtime_settings(&client, &env_bg.api_base, &token) {
                            if let Ok(mut g) = env_bg.lock_warning_sec.lock() {
                                if *g != v {
                                    log_info!(
                                        "[settings] lock_warning_sec обновлён: {} → {}",
                                        *g,
                                        v
                                    );
                                    *g = v;
                                }
                            }
                        }
                    }

                    if let Some(status) = fetch_status(&client, &env_bg.api_base, &token) {
                        let _ = proxy.send_event(UserEvent::UpdateStatus(status));
                    } else {
                        log_error!("[bg] статус не получен");
                    }
                } else {
                    log_error!("[bg] токен не найден");
                }
                tick = tick.wrapping_add(1);
                std::thread::sleep(env_bg.update_interval);
            }
        });
    }

    let tray_icon_loop = tray_icon.clone();
    let client_loop = client.clone();
    let env_loop = env.clone();

    log_debug!("[main] вхожу в event_loop");

    event_loop.run(move |event, _, control_flow| {
        *control_flow =
            ControlFlow::WaitUntil(Instant::now() + Duration::from_millis(LOOP_TICK_MS));

        if let Event::NewEvents(StartCause::ResumeTimeReached { .. })
        | Event::NewEvents(StartCause::Poll)
        | Event::NewEvents(StartCause::Init) = event
        {
            // Меню (правый клик)
            while let Ok(ev) = menu_channel.try_recv() {
                if ev.id == item_open_id {
                    log_debug!("[menu] открыть панель");
                    if agent_windows::tray_ui::is_visible() {
                        agent_windows::tray_ui::hide();
                    } else {
                        show_popup(&client_loop, &env_loop.api_base, &env_loop.token_file);
                    }
                } else if ev.id == item_pause_id {
                    log_debug!("[menu] пауза интернета");
                    if let Some(token) = read_token(&env_loop.token_file) {
                        let paused = get_last_status()
                            .map(|s| s.internet_paused)
                            .unwrap_or(false);
                        let url = if paused {
                            format!("{}/api/internet/resume", env_loop.api_base)
                        } else {
                            format!("{}/api/internet/pause", env_loop.api_base)
                        };
                        let _ = client_loop
                            .post(url)
                            .header("Authorization", format!("Bearer {}", token))
                            .send();
                    }
                } else if ev.id == item_quit_id {
                    log_debug!("[menu] выход");
                    std::process::exit(0);
                }
            }

            // Клик по иконке
            while let Ok(ev) = tray_channel.try_recv() {
                if let TrayIconEvent::Click {
                    button: tray_icon::MouseButton::Left,
                    button_state: tray_icon::MouseButtonState::Up,
                    ..
                } = ev
                {
                    log_debug!("[tray] клик по иконке");

                    if agent_windows::tray_ui::is_visible() {
                        log_debug!("[tray] окно видно — скрываю");
                        agent_windows::tray_ui::hide();
                    } else {
                        log_debug!("[tray] окно скрыто — показываю");
                        show_popup(&client_loop, &env_loop.api_base, &env_loop.token_file);
                    }
                }
            }
        }

        // Обновление статуса
        if let Event::UserEvent(UserEvent::UpdateStatus(status)) = event {
            // 🆕 Сначала обновляем LAST_STATUS — show_popup читает его.
            if let Some(m) = LAST_STATUS.get() {
                if let Ok(mut guard) = m.lock() {
                    *guard = Some(status.clone());
                }
            }

            let tooltip = build_tooltip(&status);

            if let Ok(tray) = tray_icon_loop.lock() {
                let _ = tray.set_tooltip(Some(&tooltip));
            }

            if agent_windows::tray_ui::is_visible() {
                let data = agent_windows::tray_ui::PopupData {
                    username: status.current_user.clone(),
                    screen_remaining_sec: status
                        .screen_limit_sec
                        .saturating_sub(status.screen_time_sec),
                    screen_limit_sec: status.screen_limit_sec,
                    internet_paused: status.internet_paused,
                    internet_remaining_sec: status
                        .internet_limit_sec
                        .saturating_sub(status.internet_time_sec),
                    internet_limit_sec: status.internet_limit_sec,
                    mode: status.mode.clone(),
                    is_locked: status.is_locked,
                    time_to_event_minutes: status.time_to_event.as_ref().map(|tt| tt.minutes),
                };
                agent_windows::tray_ui::update_data(data);
            }

            // 🆕 Авто-показ popup, если событие близко и мы ещё не показывали
            maybe_show_warning_popup(&status, &client_loop, &env_loop);
        }
    });
}
