import { apiGet, apiPost } from './api.js';

export const DefaultsState = () => ({
    weekDays: [],
    dailySchedule: { start_time: '08:00', end_time: '20:00', limit_min: 120, internet_limit_min: 120 },
    today: {
        start_time: '08:00', end_time: '20:00', limit_min: 120, internet_limit_min: 120,
        extra_min: 0, internet_extra_min: 0, screen_time: 0, internet_time: 0
    },
    activeToday: {
        start_time: '00:00', end_time: '23:59', limit_min: 0, internet_limit_min: 0,
        extra_min: 0, internet_extra_min: 0, screen_time: 0, internet_time: 0
    },

    async loadDefaults() {
        try {
            const d = await apiGet('/api/defaults');
            this.weekDays = d.week.map(w => ({
                name: w.name,
                day_of_week: w.day_of_week,
                start_time: d.day.start_time,
                end_time: d.day.end_time,
                limit_min: d.day.limit_min,
                internet_limit_min: d.day.internet_limit_min,
            }));
            this.dailySchedule = {
                start_time: d.day.start_time,
                end_time: d.day.end_time,
                limit_min: d.day.limit_min,
                internet_limit_min: d.day.internet_limit_min,
            };
            this.today = {
                start_time: d.day.start_time,
                end_time: d.day.end_time,
                limit_min: d.day.limit_min,
                internet_limit_min: d.day.internet_limit_min,
                extra_min: 0,
                internet_extra_min: 0,
                screen_time: 0,
                internet_time: 0,
            };
        } catch (e) {
            console.error('Не удалось загрузить defaults:', e);
        }
    },
});