import { apiGet, apiPost } from './api.js';

export const StatusState = () => ({
    global: {
        lock_message: '',
        is_locked: false,
        session_locked: false,
        mode: 'scheduled',
        mode_reason: '',
        time_to_event: null,
        api_token: '',
        current_user: '',
    },
    refreshInterval: null,

    // RDP-режим: активный пользователь не определён (SID есть, но имя неизвестно)
    isRdpUnknown() {
        return this.global.current_user === 'Неизвестно';
    },

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
            this.global.mode_reason = data.mode_reason || '';
            this.global.current_user = data.current_user || '';
            this.global.api_token = data.api_token || '';
            this.global.time_to_event = data.time_to_event ?? null;
            this.global.internet_paused = data.internet_paused || false;
            this.global.internet_blocked = data.internet_blocked || false;
            this.global.internet_paused_source = data.internet_paused_source || '';

            if (data.current_user && data.current_user !== 'Нет активных сессий' && !this.isRdpUnknown()) {
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

    // Интернет превышен по лимиту (время >= базовый лимит + поправка)
    internetExceeded() {
        const limit = this.effectiveLimitSec('internet');
        return limit > 0 && this.activeToday.internet_time >= limit;
    },

    // Кнопка «Заблокировать/Разблокировать интернет» видима, когда:
    // - нет блокировки экрана (интернет и так блокируется вместе с экраном),
    // - лимит интернета не исчерпан (иначе блокирует служба, source = "limit"),
    // - нет активной сессии (неизвестно, кого блокировать).
    showInternetToggle() {
        return !this.global.is_locked
            && !this.isRdpUnknown()
            && !!this.global.current_user
            && this.global.current_user !== 'Нет активных сессий'
            && !this.internetExceeded();
    },

    // Пауза, поставленная самим ребёнком из трея (source = "manual")
    internetPausedByUser() {
        return this.global.internet_paused
            && this.global.internet_paused_source === 'manual';
    },

    // Состояние интернета для карточки статуса
    internetStatusLabel() {
        if (this.isRdpUnknown() || !this.global.current_user
            || this.global.current_user === 'Нет активных сессий') {
            return '❓ Неизвестно';
        }
        if (this.global.is_locked) return '🔒 Заблокирован (лока)';
        if (this.internetExceeded()) return '🚫 Исчерпан лимит';
        if (this.global.internet_paused) {
            const src = this.global.internet_paused_source;
            if (src === 'admin') return '⏸ На паузе (админ)';
            if (src === 'manual') return '⏸ На паузе (пользователь)';
            return '⏸ На паузе';
        }
        return '🟢 Доступен';
    },

    internetStatusColor() {
        if (this.isRdpUnknown() || !this.global.current_user
            || this.global.current_user === 'Нет активных сессий') {
            return 'text-gray-600';
        }
        if (this.global.is_locked) return 'text-red-600';
        if (this.internetExceeded()) return 'text-red-600';
        if (this.global.internet_paused) return 'text-yellow-600';
        return 'text-green-600';
    },

    async toggleInternetBlock() {
        const url = this.global.internet_paused
            ? '/api/internet/unblock'
            : '/api/internet/block';
        try {
            const data = await apiPost(url, {});
            this.showStatus(data.message || '✅ Команда принята');
            await this.loadGlobal();
        } catch (e) {
            console.error('[toggleInternetBlock] ОШИБКА:', e);
            this.showStatus('❌ ' + e.message);
        }
    },
});
