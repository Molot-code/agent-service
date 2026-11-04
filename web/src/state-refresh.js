import { apiGet } from './api.js';

// ===== Pull-to-refresh (только мобильные) =====
// Свайп вниз от верхнего края страницы обновляет данные.
// Порог: 80px. Индикатор — фиксированный круг сверху по центру.
const PTR_THRESHOLD = 80;
const PTR_MAX = 120;

export const RefreshState = () => ({
    ptrY: 0,        // текущее смещение индикатора (0 = скрыт)
    ptrRefreshing: false, // идёт обновление (спиннер)
    _ptrStartY: null,
    _ptrActive: false,

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
            this.refreshNow().finally(() => {
                this.ptrRefreshing = false;
                this.ptrY = 0;
            });
        } else {
            this.ptrY = 0;
        }
    },

    // Обновление данных + сброс таймера авто-обновления
    async refreshNow() {
        try {
            await this.loadGlobal();
            this.startAutoRefresh();
            this.showStatus('✅ Обновлено');
        } catch (e) {
            console.error('Ошибка обновления:', e);
            this.showStatus('❌ Не удалось обновить');
        }
    },
});
