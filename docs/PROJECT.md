# Parental Control Agent (`agent-windows` v0.1.0)

> Документация для агентных моделей и разработчиков: что за программа, из чего состоит и как работает. Составлена по коду (`src/`, `config.toml`, `installer/`).

Программа родительского контроля под Windows на Rust. Состоит из трёх бинарников:
служба `ParentalControlSvc` (SQLite БД + HTTP API + HTML-панель + главный цикл контроля),
запускаемый ею **внутри сессии пользователя** блокировщик экрана `agent-cli.exe`,
и иконка в трее `agent-tray.exe`. Целевая платформа — Windows x64.

---

## 1. Архитектура — три бинарника

| Бинарник | Исходник | Контекст запуска | Что делает |
|---|---|---|---|
| `ParentalControlSvc.exe` | `src/bin/service.rs` | Windows Service (через SCM, `start=auto`) | SQLite БД, HTTP API на axum, HTML-панель, основной цикл контроля. Служба регистрируется как `ParentalControlSvc` (`config.toml [service].name`; `service.rs::install_service`). |
| `agent-cli.exe` | `src/bin/cli.rs` | **Спавнится службой через `CreateProcessAsUserW` в сессии активного пользователя** (`session::execute_in_user_session`) | Рисует собственный fullscreen-блокирующий экран (`native_lock`), после grace-периода вызывает системный `LockWorkStation`. Также команды `unlock` и `check-pin <pin>`. |
| `agent-tray.exe` | `src/bin/tray.rs` | Спавнится службой в сессии пользователя после старта API | Иконка в System Tray; опрашивает `/api/status` каждые `tray.update_interval_sec` (по умолчанию 5 сек) с токеном; попап с прогресс-индикаторами лимитов; кнопка «пауза интернета». |

Все три бинарника делят общий крейт `agent-windows` (см. `src/lib.rs`). У каждого свой глобальный логгер (`logger::init_global_logger`) c `source`: `"service"`, `"agent_cli"`, `"agent_tray"`.

### Потоки спавна

```
[Service (SYSTEM)] -- execute_in_user_session --> [agent-cli.exe в user session]
                  -- execute_in_user_session --> [agent-tray.exe в user session]
[agent-tray.exe]  -- HTTP GET  /api/status -----------> [Service :8765]
[agent-tray.exe]  -- HTTP POST /api/internet/pause|resume -> [Service]
[UI / мобильное]  -- HTTP POST /api/* ----------------> [Service]
                  -- UDP 9090 (discovery) ------------> [Service]
```

`execute_in_user_session`: `WTSGetActiveConsoleSessionId` → `WTSQueryUserToken` → `CreateEnvironmentBlock` → `CreateProcessAsUserW`. Ждёт процесс максимум 5500 мс (`WAIT_TIMEOUT` трактуется как «процесс живой — успех»); ранний выход с ненулевым кодом → `Err`.