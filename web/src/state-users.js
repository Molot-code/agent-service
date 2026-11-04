import { apiGet, apiPost } from './api.js';

export const UsersState = () => ({
    users: [],
    selectedUser: null,
    loadingUsers: false,
    loadingUserData: false,

    async loadUsers() {
        this.loadingUsers = true;
        try {
            this.users = await apiGet('/api/users');

            // 🆕 Если пользователь ещё не выбран, берем из localStorage или первого активного
            if (!this.selectedUser && this.users.length > 0) {
                const saved = localStorage.getItem('selectedUserSid');
                if (saved && this.users.some(u => u.sid === saved)) {
                    this.selectedUser = saved;
                } else {
                    const activeUser = this.users.find(u => u.is_control_enabled);
                    this.selectedUser = activeUser ? activeUser.sid : this.users[0].sid;
                }
            }
            // ❌ Убрали вызов this.loadUserData() отсюда! Его вызовет app.js
        } catch (e) {
            console.error('Ошибка загрузки пользователей:', e);
        } finally {
            this.loadingUsers = false;
        }
    },

    async loadUserData() {
        if (!this.selectedUser) return;

        // Сохраняем пользователя при загрузке его данных
        localStorage.setItem('selectedUserSid', this.selectedUser);

        this.loadingUserData = true;
        try {
            const [todayData, schedData] = await Promise.all([
                apiGet(`/api/today/${this.selectedUser}`),
                apiGet(`/api/schedule/${this.selectedUser}`),
                this.loadActivityLog()
            ]);

            // 1. Данные на сегодня
            this.today = {
                mode: todayData.mode || 'scheduled',
                manual_until: todayData.manual_until || '',
                start_time: todayData.start_time || '08:00',
                end_time: todayData.end_time || '20:00',
                limit_min: Math.floor((todayData.limit_sec ?? 7200) / 60),
                internet_limit_min: Math.floor((todayData.internet_limit_sec ?? 7200) / 60),
                extra_min: todayData.extra_time_min || 0,
                internet_extra_min: todayData.extra_internet_time_min || 0,
                screen_time: todayData.current_screen_time || 0,
                internet_time: todayData.current_internet_time || 0
            };

            // 2. Расписание
            if (schedData.schedule && Array.isArray(schedData.schedule)) {
                this.weekDays = schedData.schedule.map(day => ({
                    name: this.getWeekDayName(day.day_of_week),
                    day_of_week: day.day_of_week,
                    start_time: day.start_time,
                    end_time: day.end_time,
                    limit_min: Math.floor(day.limit_sec / 60),
                    internet_limit_min: Math.floor(day.internet_limit_sec / 60)
                }));
                this.sameSchedule = this.checkSameSchedule();
                this.syncDailyFromWeek();
            }
        } catch (e) {
            console.error('Ошибка загрузки данных пользователя:', e);
        } finally {
            this.loadingUserData = false;
        }
    },

    async toggleControl(sid, enabled) {
        try {
            await apiPost('/api/users/toggle_control', { sid, enabled });
            const user = this.users.find(u => u.sid === sid);
            if (user) user.is_control_enabled = enabled;
            this.showStatus(enabled ? '✅ Контроль включен' : '✅ Контроль отключен');
        } catch (e) {
            this.showStatus('❌ ' + e.message);
        }
    },

    async enableControlForSelectedUser() {
        if (!this.selectedUser) return;
        await this.toggleControl(this.selectedUser, true);
    },

    async returnToScheduledForUser() {
        if (!this.selectedUser) return;
        try {
            const data = await apiPost('/api/users/return_to_scheduled', { sid: this.selectedUser });
            this.showStatus(data.message || '✅ Режим «По расписанию»');
            await this.loadUserData();
        } catch (e) {
            this.showStatus('❌ ' + e.message);
        }
    },
});