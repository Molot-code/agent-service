import { apiGet, apiPost } from './api.js';

export const StatusState = () => ({
    global: {
        lock_message: '',
        is_locked: false,
        session_locked: false,
        mode: 'scheduled',
        time_to_event: null,
        api_token: '',
        current_user: '',
    },
    refreshInterval: null,

    async loadGlobal() {
        try {
            const url = this.selectedUser
                ? `/api/status?user_sid=${this.selectedUser}`
                : '/api/status';

            const data = await apiGet(url);

            this.global.lock_message = data.lock_message || '';
            this.global.is_locked = data.is_locked || false;
            this.global.session_locked = data.session_locked || false;
            this.global.mode = data.mode || 'scheduled';
            this.global.current_user = data.current_user || '';
            this.global.api_token = data.api_token || '';
            this.global.time_to_event = data.time_to_event ?? null;

            if (data.current_user && data.current_user !== 'Нет активных сессий') {
                await this.loadActiveToday(data.current_user);
            }
        } catch (e) {
            console.error('Ошибка загрузки статуса:', e);
        }
    },

    async loadActiveToday(username) {
        try {
            const user = this.users.find(u => u.username === username);
            if (!user) return;

            const data = await apiGet(`/api/today/${user.sid}`);

            this.activeToday = {
                start_time: data.start_time || '00:00',
                end_time: data.end_time || '23:59',
                limit_min: Math.floor((data.limit_sec ?? 0) / 60),
                internet_limit_min: Math.floor((data.internet_limit_sec ?? 0) / 60),
                extra_min: data.extra_time_min || 0,
                internet_extra_min: data.extra_internet_time_min || 0,
                screen_time: data.current_screen_time || 0,
                internet_time: data.current_internet_time || 0,
            };

            if (typeof this.syncExtraSnapshot === 'function') {
                this.syncExtraSnapshot();
            }
        } catch (e) {
            console.error('Ошибка загрузки today активного:', e);
        }
    },

    startAutoRefresh() {
        this.stopAutoRefresh();
        this.refreshInterval = setInterval(async () => {
            if (this.authenticated) {
                await this.loadGlobal();
            }
        }, 5000);
    },

    stopAutoRefresh() {
        if (this.refreshInterval) {
            clearInterval(this.refreshInterval);
            this.refreshInterval = null;
        }
    },

    // Эффективный лимит (базовый + extra), в секундах
    effectiveLimitSec(kind) {
        if (kind === 'screen') {
            return (this.activeToday.limit_min + this.activeToday.extra_min) * 60;
        }
        if (kind === 'internet') {
            return (this.activeToday.internet_limit_min + this.activeToday.internet_extra_min) * 60;
        }
        return 0;
    },
});