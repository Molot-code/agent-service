import { apiGet, apiPost } from './api.js';

export const UsersState = () => ({
    users: [],
    selectedUser: '',
    loadingUserData: false,

    async loadData() {
        await this.loadGlobal();
        await this.loadSettings();
        await this.loadUsers();
        this.startAutoRefresh();
    },

    async loadUsers() {
        try {
            this.users = await apiGet('/api/users');
            if (this.users.length > 0 && !this.selectedUser) {
                const activeName = this.global.current_user;
                const activeUser = (activeName && activeName !== 'Нет активных сессий')
                    ? this.users.find(u => u.username === activeName)
                    : null;
                this.selectedUser = activeUser ? activeUser.sid : this.users[0].sid;
                await this.loadUserData();
            }
        } catch (e) {
            console.error('Ошибка загрузки пользователей:', e);
        }
    },

    async loadUserData() {
        if (!this.selectedUser) return;
        this.loadingUserData = true;
        try {
            const today = await apiGet(`/api/today/${this.selectedUser}`);
            this.today = {
                start_time: today.start_time || '08:00',
                end_time: today.end_time || '20:00',
                limit_min: Math.floor((today.limit_sec ?? 7200) / 60),
                internet_limit_min: Math.floor((today.internet_limit_sec ?? 7200) / 60),
                extra_min: today.extra_time_min || 0,
                internet_extra_min: today.extra_internet_time_min || 0,
                screen_time: today.current_screen_time || 0,
                internet_time: today.current_internet_time || 0,
            };

            const { schedule } = await apiGet(`/api/schedule/${this.selectedUser}`);
            if (schedule?.length === 7) {
                schedule.forEach((day) => {
                    const feIdx = day.day_of_week === 0 ? 6 : day.day_of_week - 1;
                    const d = this.weekDays[feIdx];
                    if (!d) return;
                    d.start_time = day.start_time;
                    d.end_time = day.end_time;
                    d.limit_min = Math.floor(day.limit_sec / 60);
                    d.internet_limit_min = Math.floor(day.internet_limit_sec / 60);
                });
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
        if (!sid) return;
        try {
            await apiPost('/api/users/toggle_control', { sid, enabled });
            const u = this.users.find(u => u.sid === sid);
            if (u) u.is_control_enabled = enabled;
            await this.loadGlobal();
        } catch (e) {
            this.showStatus('❌ ' + e.message);
        }
    },

    goToUsersTab() {
        this.tab = 'users';
        if (this.global.current_user && this.global.current_user !== 'Нет активных сессий') {
            const u = this.users.find(x => x.username === this.global.current_user);
            if (u) { this.selectedUser = u.sid; this.loadUserData(); }
        }
    },

    async enableControlForSelectedUser() {
        if (this.selectedUser) await this.toggleControl(this.selectedUser, true);
    },
});