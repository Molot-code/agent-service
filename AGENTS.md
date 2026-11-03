# AGENTS.md

Карта проекта для ИИ-агента. Архитектура — в `docs/ARCHITECTURE.md`, API — в `docs/API.md`.
Здесь: **куда лезть**, **что не трогать**, **какие контракты соблюдать**.

## Стек

Rust (2021, Windows x64): `axum` 0.8, `rusqlite` (SQLite WAL), `minijinja`, `tokio`, `tower-http`.
Фронт: Alpine.js + Tailwind v4, сборка через `pnpm exec esbuild` / `pnpm exec tailwindcss` (есть `package.json`).
Три бинарника в `src/bin/`: `service.rs`, `cli.rs`, `tray.rs`. Общий крейт — `src/lib.rs`.

## Структура корня

| Путь | Что там |
|---|---|
| `src/` | **Только Rust.** Исходники фронта здесь не искать. |
| `web/` | Фронтенд: `src/` (исходники), `static/` (артефакты), `templates/` (HTML). |
| `installer/` | Inno Setup (`setup.iss`). |
| `docs/` | `ARCHITECTURE.md`, `API.md`. |
| `Taskfile.yml`, `package.json` | Сборка фронта. |
| `eslint.config.js` | Конфигурация линтера (ESLint v9 flat config). |
| `devwatch.ps1` | Скрипт для автозапуска мониторинга (не критичен для сборки). |
| `target/`, `node_modules/` | Артефакты, не трогать. |

## Структура `src/` (Rust)

| Путь | Что там |
|---|---|
| `lib.rs`, `types.rs`, `config.rs`, `error.rs`, `logger.rs` | Ядро, типы, `config.toml` (`data_dir`, `log_dir`). |
| `db/mod.rs` | `AppDb`, `new` / `new_default`, `DEFAULT_DB_DIR`. |
| `db/schema.rs` | DDL + дефолтные `settings` + `ensure_column`. **Единственное место, где CREATE/ALTER TABLE.** |
| `db/impls/settings.rs` | `get_setting`, `set_setting`, `get_state`, `set_state`, `apply_lock_status`, `apply_internet_block`. |
| `db/impls/users.rs` | Пользователи + `cleanup_stale_users` (ставит `is_active=0`). |
| `db/impls/schedule.rs`, `limits.rs`, `usage.rs` | `weekly_schedule`, `daily_limits`, `daily_usage`/`lock_events`/`activity_log`. |
| `policy.rs` | `evaluate_access`, `should_lock`, `calculate_time_to_event`, `do_sync_user_access`. |
| `api/router.rs` | Сборка `Router` (только `.route()` + группы middleware). |
| `api/server.rs` | `start_api_server`: токены, шаблоны, статика, bind, serve. |
| `api/auth.rs` | `AuthLevel`, middleware `require_auth`/`require_admin`. |
| `api/dto.rs` | Все Serde-структуры запросов/ответов. |
| `api/util.rs` | `device_name`, `resolve_manual_until`, `apply_extra`, `set_internet_blocked`. |
| `api/handlers/web.rs` | UI: `/`, `/api/status`, `/api/users`, `/api/schedule/*`, `/api/today/*`, `/api/settings`, `/api/set_param`, `/api/logs/*`. |
| `api/handlers/control.rs` | `/api/manual_action`, `/api/extra_time`, `/api/set_extra_time`, `/api/users/toggle_control`, `/api/internet/*`. |
| `api/handlers/pin.rs` | `/api/check_pin`, `/api/set_pin`, `/api/is_pin_set` (без авторизации). |
| `api/handlers/system.rs` | `/health`, `/api/defaults`. |
| `firewall.rs`, `native_lock.rs`, `session.rs`, `users.rs` | Windows: правило `ParentalControl_BlockUser_<SID>`, fullscreen-лок, `execute_in_user_session`, SID/users. |
| `bin/service.rs` | SCM, главный цикл, `apply_lock_state`, `enforce_lock_procedure`, `current_state`, `log_activity_now`. |
| `bin/cli.rs` | `lock <sec>`, `lock-system`, `unlock`, `check-pin <pin>`. |
| `bin/tray.rs` | Tao + tray-icon, опрос `/api/status`. |

**Не создавать файлы в корне `src/`** без нужды — всё разложено по доменам. `db.rs` и `api.rs` (монолиты) не существуют.

## Структура `web/`
web/src/ ← ИСХОДНИКИ (правим здесь)
web/static/ ← АРТЕФАКТЫ + иконки/шрифты (руками не правим, кроме иконок)
web/templates/ ← HTML (рендерит minijinja)

**Правила для LLM:** шаблоны — только в `web/templates/` (не в `web/src/`, не в `src/`). Исходники JS/CSS — только в `web/src/` (не в `web/static/`). Артефакты — только в `web/static/` (не в `web/src/`).

### `web/src/` — исходники (вход esbuild / tailwind)

| Путь | Что там |
|---|---|
| `app.js` | **Точка входа esbuild.** Named imports всех стейтов + мерж `...State()` в `Alpine.data('app', ...)`. **Единственное место, где перечислены стейты** — новый `state-*.js` добавлять сюда. |
| `api.js` | `apiGet` / `apiPost` с авто-подстановкой `Bearer`. |
| `format.js` | UI-хелперы (время, проценты, цвета). |
| `input.css` | **Entry Tailwind v4** (`@import "tailwindcss";` + `@source`/`@theme`). Выход — `web/static/tailwind.css`. |
| `state-auth.js` | Авторизация (PIN, логин, смена PIN). |
| `state-defaults.js` | Загрузка дефолтов (`/api/defaults`). |
| `state-status.js` | Глобальный статус, авто-обновление. |
| `state-users.js` | Список пользователей, выбор, загрузка today/schedule/activity. |
| `state-extra.js` | Слайдеры доп. времени (preview/commit). |
| `state-schedule.js` | Расписание на неделю. |
| `state-settings.js` | Глобальные настройки (табы, автосохранение). |
| `state-logs.js` | Просмотр логов. |
| `state-activity.js` | Activity log: `/api/activity_log/{sid}?date=YYYY-MM-DD`, сегменты, график (5-мин слоты), `_parseUTC`. |
| `state-manual.js` | Ручная блокировка/разблокировка. |

### `web/static/` — отдаётся браузеру

| Путь | Что там |
|---|---|
| `app.min.js`, `app.min.js.map` | **Генерируется esbuild, руками не править.** |
| `tailwind.css` | **Генерируется tailwind, руками не править.** |
| `cdn.min.js`, `cdn-collapse.min.js` | Локальные копии Alpine и Collapse (руками, не генерируются). |
| `icons/` | `favicon.png`, `favicon.svg`, `tray-*.png`. |

### `web/templates/` — HTML (minijinja)

`base.html` (layout), `index.html` (SPA), `partials/` (куски), `partials/screens/`, `partials/tabs/`.

**Правила фронтенда:**
- ES-модули (`export const ...`). `app.js` — единственная точка входа; новый `state-*.js` — только в `app.js`. Порядок: `api.js` → `format.js` → `state-*.js` → `alpine:init`.
- `input.css` — единственный entry Tailwind. Кастомные стили/`@source`/`@theme` — только здесь.
- В шаблонах подключать только `/app.min.js`, `/tailwind.css`, локальные `cdn*.js`. Остальное — через бандл.
- Не хардкодить дефолты — они с `/api/defaults`.
- **Комментарии в шаблонах**: внутри `{# ... #}` не должно быть тегов `{% ... %}`, иначе парсер ESLint (`@html-eslint/parser`) падает с ошибкой. Если нужно показать пример — используйте текст без фигурных скобок или экранируйте.

## Линтинг и проверка JS

- ESLint v9 (flat config) + `@html-eslint/parser` (пресет `NUNJUCKS` для совместимости с MiniJinja) + `eslint-plugin-alpinejs`.
- **Запуск**: `pnpm run lint` (только проверка), `pnpm run lint:fix` (автоисправление).
- Плагин Alpine проверяет только одно правило: `alpinejs/no-raw-dom-access` (запрет прямого доступа к DOM). Он **не** валидирует синтаксис `x-data`/`x-on` и не заменяет TypeScript.
- Синтаксические ошибки JS (например, `inport`) ловит **esbuild** на этапе сборки, а не ESLint.
- Для поиска дублей и неиспользуемого кода используйте отдельные CLI-инструменты (`jscpd`, `dead-code-hunter`), не встроенные в `Taskfile.yml`.

## Правила «куда лезть»

| Задача | Файл |
|---|---|
| Блокировка экрана | `src/bin/service.rs::apply_lock_state`, `enforce_lock_procedure` |
| Политика доступа | `src/policy.rs::check_screen_time_policy`, `should_lock` |
| Новое поле в TodayLimit | `src/types.rs::TodayLimit` → `src/db/impls/limits.rs` → `src/api/dto.rs` |
| Новый эндпоинт | `src/api/handlers/*.rs` + `src/api/router.rs` |
| Блокировка интернета | `src/firewall.rs` + `src/bin/service.rs::apply_lock_state` |
| Трей | `src/bin/tray.rs` + `src/tray_ui.rs::PopupData` |
| Схема БД | **только** `src/db/schema.rs` (+ `ensure_column`) |
| Дефолты настроек | `src/db/schema.rs` (INSERT OR IGNORE) + `config.toml [limits]` |
| «До конца дня» | `src/api/util.rs::resolve_manual_until` |
| Поле в UI (логика) | `web/src/state-*.js` |
| Поле в UI (верстка) | `web/templates/index.html` / `partials/*.html` |
| Activity log (запись) | `src/db/impls/usage.rs::log_activity` + вызовы в `src/bin/service.rs`, `src/api/handlers/control.rs` |
| Activity log (график UI) | `web/src/state-activity.js` |
| Смена `state` в графике | `src/activity.rs::current_state` (уточнить: `src/bin/activity.rs` или `src/activity.rs`) |
| Логи (чтение) | `src/api/handlers/web.rs::list_log_dates`, `get_logs` |
| Стили (Tailwind) | `web/src/input.css` → `web/static/tailwind.css` |
| Сборка бандла | `web/src/app.js` → `web/static/app.min.js` |
| Линтинг JS/HTML | `eslint.config.js`, `pnpm run lint` |

## Контракты API (не ломать без согласования)

- `Authorization: Bearer <token>`. Уровни: `api_token`/`admin_pin` → Admin; `tray_token` → Tray (только `/api/status`, `/api/internet/*`).
- Middleware `require_auth` (Tray+) / `require_admin` (Admin) в `src/api/router.rs`. Хендлеры **не** проверяют токен вручную.
- Без авторизации: `/api/check_pin`, `/api/set_pin`, `/api/is_pin_set`, `/api/defaults`.
- `POST /api/check_pin` → `{pin: "..."}`.
- `POST /api/manual_action` → `until: "YYYY-MM-DD HH:MM:SS"` или `"-1"`.
- `GET /api/status` → `StatusResponse`; `GET /api/defaults` → `DefaultsResponse` (источник правды дефолтов UI).
- `GET /api/activity_log/{sid}?date=YYYY-MM-DD` → `ActivityLogResponse` (до 1000 событий за локальную дату). Поле `state` обязательно (значения — см. «Контракты БД»).

## Контракты БД

- Все запросы — через `AppDb`; напрямую `rusqlite::Connection` не использовать.
- Горячие запросы — `prepare_cached`. Вставка >1 строки — в транзакции (`unchecked_transaction`).
- Секреты (`admin_pin`, `api_token`, `tray_token`) — в `settings` с `is_secret=1`.
- `users.control_enabled` — рабочий флаг; `users.is_controlled` — legacy.
- `users.is_active` — ставится в `0` через `cleanup_stale_users`; миграция — `ensure_column`.
- `activity_log` — журнал событий (блокировки, разблокировки, heartbeat).
- **`activity_log.state`** ∈ {`active_scheduled`, `active_manual`, `no_internet`, `screen_locked`, `session_locked`}; `''` = нет данных.
- **`activity_log.created_at`** — **всегда UTC** (`datetime('now')`). Конвертация в локальное — на фронте (`new Date(str + 'Z')` или `_parseUTC`).

## Запреты

- Не добавлять SQL DDL вне `src/db/schema.rs`.
- Не дублировать `check_auth` в хендлерах — только middleware.
- Не создавать `Router::new()` вне `src/api/router.rs`.
- Не писать в `system_state` из API-хендлеров (кроме `internet_blocked`, `lock_status`) — зона `src/bin/service.rs`.
- Не менять сигнатуры публичных методов `AppDb` без обновления вызовов (`rusqlite::Result<...>`).
- **Не править `web/static/app.min.js` и `web/static/tailwind.css` руками** — перезаписываются сборкой.
- **Не класть исходники в `web/static/`** и **артефакты в `web/src/`**.
- **Не искать шаблоны в `web/src/` или `src/`** — только `web/templates/`.
- **Не класть `input.css` в `src/` (Rust)** — его место `web/src/`.
- **Не добавлять второй aggregator (`entry.js`)** — точка входа одна: `web/src/app.js`.
- **Не подключать скрипты/стили в шаблонах напрямую** (кроме `/app.min.js`, `/tailwind.css`, `cdn*.js`).
- **Не использовать `{% ... %}` внутри `{# ... #}` в шаблонах** — ломает парсер ESLint.
- После правок в `web/src/**` или `web/templates/**` запускать **`task dev-frontend`**, а не `task js` (последний не доставляет файлы и не перезапускает службу).

## Сборка и запуск

- `cargo build` — быстрая проверка; `cargo build --release` — прод.
- `cargo test template_tests` — проверка шаблонов minijinja в строгом режиме (`UndefinedBehavior::Strict`, `path_loader("web/templates")`). Ловит серверные `{{ }}` вокруг Alpine-геттеров и другие undefined-переменные. Запускать после правок в `web/templates/**`.
- `task styles` — `web/src/input.css` → `web/static/tailwind.css` (через `pnpm exec tailwindcss`).
- `task js` — **только сборка** `web/src/app.js` → `web/static/app.min.js` (через `pnpm exec esbuild`) + `pnpm run lint`. Не доставляет файлы и не перезапускает службу. Для проверки правок UI **не использовать**.
- `task dev-frontend` — **основная команда после любых правок UI**: сборка фронта (`styles` + `js`) + копирование в `C:\Program Files\ParentalControl` + перезапуск службы. Без установщика и без пересборки Rust.
- `task dev` — как `dev-frontend`, но дополнительно пересобирает Rust (`cargo build --release`). Использовать, если менялись `.rs`.
- `task dev-watch` — watch-режим: пересборка фронта и перезапуск службы при изменениях в `web/src/**` и `web/templates/**`.
- `task` — полный пайплайн + Inno Setup. `backend` зависит от `styles`/`js`; `installer` использует `{{.ROOT_DIR}}` (запускать можно из любой подпапки).
- `pnpm run lint` — проверка JS и HTML через ESLint.
- Логи: `C:\ProgramData\ParentalControl\logs\YYYY-MM-DD\{service|agent_cli|agent_tray|debug|error}.log`.
- БД и токены: `C:\ProgramData\ParentalControl\{data.db|api_token.txt|tray_token.txt}`.

## Что где подробнее

- Бинарники, SCM, WTS, firewall per-user → `docs/ARCHITECTURE.md`.
- Таблица эндпоинтов → `docs/API.md`.
- Установка/uninstall → `installer/setup.iss`.