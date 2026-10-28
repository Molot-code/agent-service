import { apiGet, apiPost } from './api.js';

export const LogsState = () => ({
    logsDate: '',
    logsSource: 'service',
    logsLevel: 'info',
    logsDates: [],
    logsContent: '',

    async loadLogDates() {
        try {
            this.logsDates = await apiGet('/api/logs');
            if (this.logsDates.length > 0 && !this.logsDate) {
                this.logsDate = this.logsDates[0];
            }
        } catch (e) {
            console.error('Ошибка загрузки дат логов:', e);
        }
    },

    async loadLogs() {
        if (!this.logsDate) return;
        try {
            const url = `/api/logs/${this.logsDate}?source=${this.logsSource}&level=${this.logsLevel}&limit=1000`;
            const data = await apiGet(url);
            this.logsContent = data.lines.join('\n') || '(пусто)';
        } catch (e) {
            this.logsContent = '❌ Ошибка загрузки логов';
        }
    },
});