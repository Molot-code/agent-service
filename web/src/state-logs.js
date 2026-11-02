import { apiGet } from './api.js';

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
                await this.loadLogs(); // Сразу грузим логи за первую дату
            } else if (this.logsDates.length === 0) {
                this.logsContent = '⚠️ Нет доступных логов (папка пуста)';
            }
        } catch (e) {
            console.error('Ошибка загрузки дат логов:', e);
            this.logsContent = '❌ Ошибка получения списка дат';
        }
    },

    async loadLogs() {
        if (!this.logsDate) {
            this.logsContent = '⚠️ Не выбрана дата. Нажмите "Обновить" или выберите дату.';
            return;
        }
        this.logsContent = 'Загрузка...';
        try {
            const url = `/api/logs/${this.logsDate}?source=${this.logsSource}&level=${this.logsLevel}&limit=1000`;
            const data = await apiGet(url);
            if (data.lines && data.lines.length > 0) {
                this.logsContent = data.lines.join('\n');
            } else {
                this.logsContent = '(файл пуст или не найден)';
            }
        } catch (e) {
            this.logsContent = '❌ Ошибка загрузки логов: ' + e.message;
        }
    },
});