import { apiGet } from './api.js';

// ===== Pull-to-refresh (только мобильные) =====
// Свайп вниз от верхнего края страницы обновляет данные.
// Порог: 80px. Индикатор — фиксированный круг сверху по центру.
const PTR_THRESHOLD = 80;
const PTR_MAX = 120;

// ===== Просыпание мобильного клиента (WebView) =====
// WebView уходит на http://ip:port/ (window.location.assign), лаунчерный JS
// выгружается, Tauri IPC на remote origin недоступен. Обновление после сна —
// целиком на этой странице, через браузерные события.
//
// Триггеры (все ведут в refreshNow, дедуп не чаще раза в RESUME_MIN_INTERVAL):
//   visibilitychange → visible   (не гарантирован при блокировке кнопкой питания)
//   pageshow                        (bfcache в Android WebView нет, но событие может прийти)
//   focus                         (страховка)
//   online                        (возврат сети)
//   setInterval(RESUME_POLL_MS)   (страховка, если ни одно событие не пришло)
//
// Сброс _lastResumeRefresh при hidden / pageshow — первое событие после
// возврата из background всегда проходит, даже если poll тикал за секунду до сна.
const RESUME_MIN_INTERVAL = 3000; // дедуп: не чаще раза в 3 сек
const RESUME_POLL_MS = 5000;      // poll: loadGlobal + loadSettings + тик _now каждые 5 сек
const STALE_TTL = 15000;          // 15с = 3× poll (5с) — не мигает при живом сервере
const SERVER_DOWN_THRESHOLD = 1;  // оверлей «Сервер недоступен» с первой ошибки

export const RefreshState = () => ({
    ptrY: 0,        // текущее смещение индикатора (0 = скрыт)
    ptrRefreshing: false, // идёт обновление (спиннер)
    _ptrStartY: null,
    _ptrActive: false,

    // ===== Stale / TTL =====
    lastUpdated: 0,   // timestamp последнего успешного loadGlobal() (0 = данных ещё нет)
    stale: false,     // true, если последний loadGlobal() упал (сервер недоступен)
    _now: Date.now(), // реактивное "сейчас" — обновляется poll-тиком, isStale() читает его

    // ===== Дедуп / inFlight =====
    _lastResumeRefresh: 0,
    _refreshInFlight: false,
    _refreshSeq: 0,
    _resumeInitialized: false,
    _resumePoll: null,

    // ===== Сервер недоступен =====
    serverUnavailable: false,  // true → блокирующий оверлей
    _consecutiveErrors: 0,     // счётчик ошибок подряд (сбрасывается при успехе)

    // Данные устарели: либо последний запрос упал, либо прошло больше STALE_TTL.
    // Читает реактивный _now → Alpine сам перерисовывает x-show="isStale()".
    isStale() {
        if (this.stale) return true;
        if (this.lastUpdated === 0) return false; // данных ещё нет — отдельный вид плашки
        return this._now - this.lastUpdated > STALE_TTL;
    },

    hasData() {
        return this.lastUpdated > 0;
    },

    lastUpdatedLabel() {
        if (!this.hasData()) return '';
        const d = new Date(this.lastUpdated);
        const p = (n) => String(n).padStart(2, '0');
        return `${p(d.getHours())}:${p(d.getMinutes())}:${p(d.getSeconds())}`;
    },

    onPtrTouchStart(e) {
        // Только мобильные, только один палец, только если страница наверху
        if (!this.isMobileView() || e.touches.length !== 1) return;
        if (window.scrollY > 0 || document.documentElement.scrollTop > 0) return;
        this._ptrStartY = e.touches[0].clientY;
        this._ptrActive = false;
    },

    onPtrTouchMove(e) {
        if (this._ptrStartY === null) return;
        const dy = e.touches[0].clientY - this._ptrStartY;

        // Тянем вниз от верхнего края — включаем режим pull
        if (!this._ptrActive && dy > 0) {
            this._ptrActive = true;
        }
        if (!this._ptrActive) return;

        // Резинка: смещение растёт медленнее пальца
        this.ptrY = Math.min(PTR_MAX, dy * 0.5);
    },

    onPtrTouchEnd() {
        if (this._ptrStartY === null) return;
        const shouldRefresh = this._ptrActive && this.ptrY >= PTR_THRESHOLD;
        this._ptrStartY = null;
        this._ptrActive = false;

        if (shouldRefresh) {
            this.ptrY = PTR_MAX; // удерживаем спиннер на месте
            this.ptrRefreshing = true;
            this.maybeRefresh('pull').finally(() => {
                this.ptrRefreshing = false;
                this.ptrY = 0;
            });
        } else {
            this.ptrY = 0;
        }
    },

    // Обновление данных + сброс таймера авто-обновления.
    // Дедуп: не чаще RESUME_MIN_INTERVAL, один запрос в полёте,
    // устаревшие ответы отбрасываются по seq.
    async refreshNow(reason, force = false) {
        const now = Date.now();
        if (!force && (this._refreshInFlight || now - this._lastResumeRefresh < RESUME_MIN_INTERVAL)) return;
        this._lastResumeRefresh = now;
        const mySeq = ++this._refreshSeq;
        this._refreshInFlight = true;
        this.ptrRefreshing = true;
        try {
            await this.loadGlobal();
            await this.loadSettings(); // настройки могут меняться извне (CLI, другой клиент)
            if (mySeq !== this._refreshSeq) return; // устаревший ответ — отбрасываем
            // Успех: сбрасываем счётчик ошибок, скрываем оверлей.
            this._consecutiveErrors = 0;
            this.serverUnavailable = false;
            // Poll — тихий: не показываем «Обновлено» каждые 5 сек.
            // Остальные триггеры (visibility/pageshow/focus/online/pull) — видимые.
            if (reason !== 'poll') {
                this.showStatus('✅ Обновлено');
            }
        } catch (e) {
            if (mySeq !== this._refreshSeq) return;
            console.error('Ошибка обновления:', e);
            this._consecutiveErrors++;
            // Оверлей «Сервер недоступен» после SERVER_DOWN_THRESHOLD ошибок подряд.
            if (this._consecutiveErrors >= SERVER_DOWN_THRESHOLD) {
                this.serverUnavailable = true;
            }
            if (reason !== 'poll') {
                this.showStatus('❌ Не удалось обновить');
            }
        } finally {
            if (mySeq === this._refreshSeq) {
                this._refreshInFlight = false;
                this.ptrRefreshing = false;
                this.ptrY = 0;
            }
        }
    },

    // Точка входа всех триггеров просыпания.
    // Гейт ТОЛЬКО visibilityState: на серверной странице authenticated не существует
    // (лаунчерный JS выгружен после window.location.assign).
    maybeRefresh(reason) {
        if (document.visibilityState !== 'visible') return;
        this.refreshNow(reason);
    },

    // Кнопка «Попробовать снова» в оверлее: сбрасываем дедуп и пробуем.
    // Оверлей НЕ скрываем заранее — refreshNow() сам решит:
    //   успех → serverUnavailable = false (оверлей исчезнет)
    //   ошибка → serverUnavailable остаётся true (оверлей на месте)
    async retryServer() {
        this._lastResumeRefresh = 0; // сброс дедупа — запрос пройдёт сразу
        await this.refreshNow('retry', true); // force — обойти дедуп, даже если poll в полёте
    },

    // Один раз при старте (вызывается из loadData в app.js).
    initResumeListeners() {
        if (this._resumeInitialized) return;
        this._resumeInitialized = true;

        // Сброс дедупа при уходе в background: первый refresh после возврата
        // всегда проходит, даже если poll тикал за секунду до сна.
        const onHidden = () => { this._lastResumeRefresh = 0; };

        document.addEventListener('visibilitychange', () => {
            if (document.visibilityState === 'hidden') {
                onHidden();
            } else {
                this.maybeRefresh('visibility');
            }
        });
        window.addEventListener('pageshow', () => {
            onHidden();
            this.maybeRefresh('pageshow');
        });
        window.addEventListener('focus', () => this.maybeRefresh('focus'));
        window.addEventListener('online', () => this.maybeRefresh('online'));

        // Poll-страховка (если ни одно событие не пришло) + тик реактивного _now,
        // чтобы isStale() сам перерисовался по TTL без внешнего рендера.
        this._resumePoll = setInterval(() => {
            this._now = Date.now();
            this.maybeRefresh('poll');
        }, RESUME_POLL_MS);
    },
});
