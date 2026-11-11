import { apiGet, apiPost } from './api.js';

export const ExtraState = () => ({
    extraSnapshot: { screen: 0, internet: 0 },

    syncExtraSnapshot() {
        this.extraSnapshot.screen = this.activeToday.extra_min || 0;
        this.extraSnapshot.internet = this.activeToday.internet_extra_min || 0;
    },

    previewExtra(kind, value) {
        const v = Number(value) || 0;
        if (kind === 'screen') {
            this.activeToday.extra_min = v;
            // Односторонняя зависимость: если поправки были равны,
            // интернет следует за экраном (экран за интернетом — нет)
            if (this.extraSnapshot.screen === this.extraSnapshot.internet &&
                this.extraSnapshot.screen !== v) {
                this.activeToday.internet_extra_min = v;
            }
        } else {
            this.activeToday.internet_extra_min = v;
        }
    },

    async commitExtra(kind) {
        const current = kind === 'screen' ? this.activeToday.extra_min : this.activeToday.internet_extra_min;
        const previous = this.extraSnapshot[kind] || 0;
        if (current === previous) return;

        const user = this.users.find(u => u.username === this.global.current_user);
        if (!user) { this.showStatus('❌ Активный пользователь не найден'); return; }

        // Односторонняя синхронизация: если поправки были равны ДО изменения
        // и меняется экран — интернет следует за экраном (и наоборот — нет)
        const syncInternet = kind === 'screen'
            && this.extraSnapshot.screen === this.extraSnapshot.internet;

        const body = { sid: user.sid };
        if (kind === 'screen') body.screen_minutes = current;
        if (kind === 'internet') body.internet_minutes = current;
        if (syncInternet) body.internet_minutes = current;

        try {
            await apiPost('/api/set_extra_time', body);
            this.extraSnapshot[kind] = current;
            if (syncInternet) this.extraSnapshot.internet = current;
        } catch (e) {
            this.showStatus('❌ ' + e.message);
        }
    },

    async nudgeExtra(kind, step) {
        const current = kind === 'screen' ? this.activeToday.extra_min : this.activeToday.internet_extra_min;
        const next = Math.max(-60, Math.min(60, current + step));
        this.previewExtra(kind, next);
        await this.commitExtra(kind);
    },

    async addExtraTime(key, minutes, opts = {}) {
        if (!key || !minutes) return;
        const user = this.users.find(u => u.username === this.global.current_user);
        if (!user) { this.showStatus('❌ Активный пользователь не найден'); return; }

        const body = { sid: user.sid };
        if (key === 'screen') body.screen_minutes = minutes;
        if (key === 'internet') body.internet_minutes = minutes;

        try {
            await apiPost('/api/extra_time', body);
            if (!opts.silent) {
                if (key === 'screen') this.activeToday.extra_min += minutes;
                if (key === 'internet') this.activeToday.internet_extra_min += minutes;
            } else {
                this.syncExtraSnapshot();
            }
        } catch (e) {
            this.showStatus('❌ ' + e.message);
        }
    },

    async resetExtraTime(kind) {
        this.previewExtra(kind, 0);
        await this.commitExtra(kind);
    },
});