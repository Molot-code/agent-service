# Parental Control (agent-windows)

Система родительского контроля для Windows x64: ограничивает экранное время и интернет
для пользователей по расписанию, поддерживает ручные блокировки, поправки лимитов,
блокировку экрана и интернета, веб-панель управления, трей и CLI.

## Возможности

- **Расписание** — индивидуальные окна доступа (день/время) для каждого пользователя.
- **Лимиты** — экранное время и интернет-время за день, поправки (extra time) ±60 мин.
- **Ручное управление** — блокировка/разблокировка на N минут или до конца дня, возврат в режим расписания.
- **Блокировка экрана** — fullscreen-оверлей в сессии пользователя (через `agent-cli.exe`).
- **Блокировка интернета** — правило Windows Firewall на пользователя (per-SID).
- **Веб-панель** — статус, пользователи, расписание, activity log, настройки, логи.
- **Трей** — мини-статус и управление из системного трея.
- **PIN-авторизация** — защита веб-панели и API.

## Стек

- **Бэкенд**: Rust 2021, Windows x64. `axum` 0.8 (HTTP API), `rusqlite` (SQLite WAL),
  `minijinja` (шаблоны), `tokio`, `tower-http`, `windows-service` (SCM),
  `windows` (Win32 API), `windows_firewall`, `tray-icon` + `tao` (трей).
- **Фронтенд**: Alpine.js + Tailwind CSS v4, сборка `esbuild` + `tailwindcss` CLI,
  управление зависимостями через `pnpm`.
- **Установщик**: Inno Setup 7 (`installer/setup.iss`).

## Структура проекта

```
├── src/                  # Rust (только бэкенд)
│   ├── bin/
│   │   ├── service.rs    # служба (SCM, главный цикл, apply_lock_state)
│   │   ├── cli.rs        # agent-cli: lock / unlock / lock-system
│   │   └── tray.rs       # agent-tray: трей (tao + tray-icon)
│   ├── api/              # HTTP API (router, auth, dto, handlers)
│   ├── db/               # SQLite: schema.rs (DDL), impls/*
│   ├── policy.rs         # политика доступа (evaluate_access, should_lock)
│   ├── firewall.rs       # правило ParentalControl_BlockUser_<SID>
│   ├── native_lock.rs    # fullscreen-блокировка экрана
│   ├── session.rs        # сессии Windows, execute_in_user_session
│   └── ...               # types, config, error, logger
├── web/
│   ├── src/              # исходники фронта (app.js — точка входа, state-*.js, input.css)
│   ├── static/           # АРТЕФАКТЫ (app.min.js, tailwind.css) + иконки — руками не править
│   └── templates/        # HTML-шаблоны minijinja (base.html, index.html, partials/)
├── installer/setup.iss   # Inno Setup
├── Taskfile.yml          # task-задачи сборки
├── config.toml           # конфигурация (порт, пути, дефолтные лимиты)
├── eslint.config.js      # ESLint v9 (flat config)
└── package.json          # pnpm-зависимости фронта
```

## Тройка бинарников

| Бинарник | Назначение |
|---|---|
| `ParentalControlSvc.exe` | Служба Windows: API на порту 8765, главный цикл (тик 1 с), блокировки, запуск CLI/трея в сессии пользователя. |
| `agent-cli.exe` | Запускается службой в сессии пользователя: `lock <sec>`, `unlock`, `lock-system`, `check-pin <pin>`. |
| `agent-tray.exe` | Трей: опрашивает `/api/status` раз в 5 с, показывает статус и кнопки управления. |

## Сборка

Требуется: Rust (stable, x64), Node.js + `pnpm`, `task` (go-task), Inno Setup 7 (для установщика).

```powershell
pnpm install        # зависимости фронта
task                # полный цикл: фронт + cargo build --release + Inno Setup
```

Полезные задачи:

| Команда | Что делает |
|---|---|
| `task dev-frontend` | **Основная команда после правок UI**: сборка фронта + копирование в `C:\Program Files\ParentalControl` + перезапуск службы. Rust не пересобирается. |
| `task dev` | Как `dev-frontend`, но дополнительно `cargo build --release`. Использовать, если менялись `.rs`. |
| `task dev-watch` | Watch-режим: пересборка фронта и перезапуск службы при изменениях в `web/src/**`, `web/templates/**`. |
| `task styles` | `web/src/input.css` → `web/static/tailwind.css`. |
| `task js` | Только сборка `web/src/app.js` → `web/static/app.min.js` + lint. Не доставляет файлы. |
| `pnpm run lint` | ESLint проверка JS и HTML. |
| `cargo check` | Быстрая проверка Rust. |
| `cargo test template_tests` | Проверка шаблонов minijinja в строгом режиме (запускать после правок `web/templates/**`). |

## Установка

1. `task` (или `task installer`) — соберёт `installer\output\ParentalControlSetup.exe`.
2. Запустить установщик (нужны права администратора):
   - копирует бинарники, `config.toml`, `static/`, `templates/` в `C:\Program Files\ParentalControl`;
   - создаёт `C:\ProgramData\ParentalControl` с полными правами для пользователей;
   - открывает в брандмауэре TCP 8765 (API) и UDP 9090 (обнаружение);
   - регистрирует и запускает службу `ParentalControlSvc` (автозапуск).

### Ключевые пути

| Путь | Содержимое |
|---|---|
| `C:\Program Files\ParentalControl\` | бинарники, `config.toml`, `static/`, `templates/` |
| `C:\ProgramData\ParentalControl\data.db` | SQLite (WAL) |
| `C:\ProgramData\ParentalControl\api_token.txt` | API-токен (Admin) |
| `C:\ProgramData\ParentalControl\tray_token.txt` | Токен трея |
| `C:\ProgramData\ParentalControl\logs\YYYY-MM-DD\` | логи: `service`, `agent_cli`, `agent_tray`, `debug`, `error` |

## Конфигурация (`config.toml`)

```toml
[service]
name = "ParentalControlSvc"
api_port = 8765            # порт HTTP API
tick_duration_secs = 1     # период главного цикла
log_level = "debug"

[tray]
host = "127.0.0.1"
update_interval_sec = 5    # период опроса /api/status

[paths]
data_dir = "C:\\ProgramData\\ParentalControl"
log_dir  = "C:\\ProgramData\\ParentalControl\\logs"
static_dir = "static"

[limits]
default_limit_sec  = 7200  # дефолтный лимит экранного времени (сек)
default_start_time = "08:00"
default_end_time   = "20:00"
```

## API (кратко)

- Авторизация: `Authorization: Bearer <token>`.
  - `api_token` / `admin_pin` → уровень Admin (все эндпоинты).
  - `tray_token` → уровень Tray (только `/api/status`, `/api/internet/*`).
- Без авторизации: `/api/check_pin`, `/api/set_pin`, `/api/is_pin_set`, `/api/defaults`.
- Основные:
  - `GET /api/status` — статус системы (режим, блокировки, интернет, следующий event).
  - `GET /api/users`, `GET /api/today/{sid}`, `GET /api/schedule/{sid}`.
  - `POST /api/manual_action` — `until: "YYYY-MM-DD HH:MM:SS"` или `"-1"` (до конца дня).
  - `POST /api/extra_time` / `POST /api/set_extra_time` — поправки (add/set), поля `screen_minutes`, `internet_minutes` (необязательные).
  - `POST /api/internet/block` / `POST /api/internet/unblock`.
  - `GET /api/activity_log/{sid}?date=YYYY-MM-DD` — журнал событий (до 1000 за локальную дату).
  - `GET /api/defaults` — дефолты для UI (источник правды).
- Полная таблица — в `docs/API.md` (если актуализирована).

## Правила разработки (коротко)

- **Схема БД** — только `src/db/schema.rs` (+ `ensure_column`).
- **Новый эндпоинт** — `src/api/handlers/*.rs` + регистрация в `src/api/router.rs`.
- **Авторизация** — только middleware `require_auth` / `require_admin`; в хендлерах не проверять.
- **Фронт**: исходники — `web/src/` (ES-модули, точка входа `app.js`), шаблоны — `web/templates/`,
  артефакты `web/static/app.min.js` и `web/static/tailwind.css` руками не править.
- После правок в `web/src/**` или `web/templates/**` — `task dev-frontend` (не `task js`).
- `activity_log.created_at` — всегда UTC; конвертация в локальное — на фронте.
- Детальная карта «куда лезть» и запреты — в `AGENTS.md`.

## Лицензия

Приватный проект.
