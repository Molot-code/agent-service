// src/bin/cli.rs
use std::{thread, time::Duration};

use agent_windows::{
    config::Config, db::AppDb, log_debug, log_error, log_warn, native_lock,
    policy::evaluate_access,
};

fn main() {
    // CLI запускается из службы в сессии пользователя.
    // Инициализируем свой логгер с source="agent_cli", уровень — из config.toml.
    let config = Config::new();
    let logs_path = std::path::Path::new(&config.paths.log_dir);
    agent_windows::logger::init_global_logger(
        &logs_path.to_string_lossy(),
        "agent_cli",
        agent_windows::logger::LogLevel::from_str(&config.service.log_level),
        14,
    );

    let args: Vec<String> = std::env::args().collect();

    if args.len() <= 1 {
        log_warn!(
            "Использование: agent-cli.exe [lock <delay_sec>|unlock|lock-system]"
        );
        return;
    }

    match args[1].as_str() {
        "lock" => {
            let delay_sec: u64 = args
                .get(2)
                .and_then(|s| s.parse::<u64>().ok())
                .map(|v| v.clamp(1, 300))
                .unwrap_or(5);

            log_debug!("🔒 [CLI] Инициируем блокировку (grace: {}с)...", delay_sec);

            // 🆕 Читаем lock_message из БД, подставляем {} в расчётное время и передаём в native_lock
            match AppDb::new(&config.paths.data_dir) {
                Ok(db) => {
                    if let Ok(Some(msg)) = db.get_setting("lock_message") {
                        if !msg.is_empty() {
                            // Вычисляем время до следующего события через policy
                            // evaluate_access(None) сам берёт активного пользователя
                            let time_str = if let Some(state) = evaluate_access(&db, None) {
                                format_time_to_event(&state.policy.time_to_event)
                            } else {
                                log_debug!("[CLI] evaluate_access вернул None — нет активного пользователя");
                                String::new()
                            };

                            // Подставляем {} в шаблон сообщения
                            let final_msg = if time_str.is_empty() {
                                msg
                            } else {
                                msg.replace("{}", &time_str)
                            };

                            log_debug!("[CLI] lock_message загружен из БД: {}", final_msg);
                            native_lock::set_lock_message(final_msg);
                        }
                    }
                }
                Err(e) => {
                    log_warn!("[CLI] Не удалось открыть БД для lock_message: {}", e);
                }
            }

            let _ = native_lock::lock_screen();

            let mut attempts = 0;
            while !native_lock::is_locked() && attempts < 50 {
                thread::sleep(Duration::from_millis(100));
                attempts += 1;
            }

            if !native_lock::is_locked() {
                log_error!(
                    "❌ [CLI] КРИТИЧЕСКАЯ ОШИБКА: Кастомная блокировка не активировалась за 5 сек!",
                );
                std::process::exit(1);
            }

            log_debug!("✅ [CLI] Кастомный экран активен.");

            let grace_period = Duration::from_secs(delay_sec);
            let start = std::time::Instant::now();
            let mut user_unlocked_early = false;

            while start.elapsed() < grace_period {
                if !native_lock::is_locked() {
                    user_unlocked_early = true;
                    break;
                }
                thread::sleep(Duration::from_millis(200));
            }

            if user_unlocked_early {
                log_debug!(
                    "🔓 [CLI] Пользователь разблокировал экран в течение grace-периода. Системная блокировка не требуется.",
                );
                return;
            }

            log_debug!(
                "⏰ [CLI] Grace-период истёк. Выполняю системную блокировку (LockWorkStation)...",
            );

            match agent_windows::session::lock_session() {
                Ok(_) => {
                    log_debug!("✅ [CLI] Системная блокировка отправлена. Завершаю CLI.");
                }
                Err(e) => {
                    log_error!(
                        "❌ [CLI] Системная блокировка не удалась: {}. Держу кастомный экран как fallback.",
                        e
                    );
                    while native_lock::is_locked() {
                        thread::sleep(Duration::from_millis(500));
                    }
                    log_debug!("🔓 [CLI] Кастомный экран снят пользователем.");
                }
            }
        }

        "lock-system" => {
            log_debug!("🔒 [CLI] Принудительная системная блокировка (lock-system)...");
            match agent_windows::session::lock_session() {
                Ok(_) => {
                    log_debug!("✅ [CLI] Системная блокировка выполнена.");
                }
                Err(e) => {
                    log_error!("❌ [CLI] Ошибка LockWorkStation: {}", e);
                    std::process::exit(1);
                }
            }
        }

        "unlock" => {
            log_debug!("🔓 [CLI] Принудительная разблокировка...");
            let _ = native_lock::unlock_screen();
        }

        _ => {
            log_warn!("Неизвестная команда: {}", args[1]);
        }
    }
}

/// Форматирует TimeToEvent в человекочитаемый вид: "15ч 23мин" или "45мин"
fn format_time_to_event(tte: &agent_windows::types::TimeToEvent) -> String {
    match tte {
        agent_windows::types::TimeToEvent::UnlockIn { minutes, .. } => format_minutes(*minutes),
        agent_windows::types::TimeToEvent::LockIn { minutes, .. } => format_minutes(*minutes),
    }
}

/// Форматирует минуты в "Xч Yмин" или "Yмин"
fn format_minutes(minutes: u32) -> String {
    if minutes == 0 {
        "0 мин".to_string()
    } else if minutes < 60 {
        format!("{} мин", minutes)
    } else {
        let h = minutes / 60;
        let m = minutes % 60;
        if m == 0 {
            format!("{}ч", h)
        } else {
            format!("{}ч {}мин", h, m)
        }
    }
}
