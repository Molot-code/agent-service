import { apiGet, apiPost } from './api.js';

export const ManualState = () => ({
    showLockMenu: false,

    formatDateTimeFromOffset(minutes) {
        const d = new Date(Date.now() + minutes * 60 * 1000);
        const pad = (n) => String(n).padStart(2, '0');
        return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ` +
            `${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`;
    },

    async manualAction(action, until) {
        this.showLockMenu = false;
        try {
            const data = await apiPost('/api/manual_action', { action, until });
            this.showStatus(data.message || '✅ Команда принята');
            await this.loadGlobal();
        } catch (e) {
            this.showStatus('❌ ' + e.message);
        }
    },

    lockUntilEndOfDay() { this.manualAction('lock', '-1'); },
    lockForMinutes(m) { this.manualAction('lock', this.formatDateTimeFromOffset(m)); },
    unlockUntilEndOfDay() { this.manualAction('unlock', '-1'); },
    unlockForMinutes(m) { this.manualAction('unlock', this.formatDateTimeFromOffset(m)); },
});