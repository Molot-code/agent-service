import { AuthState } from './state-auth.js';
import { StatusState } from './state-status.js';
import { DefaultsState } from './state-defaults.js';
import { UsersState } from './state-users.js';
import { ExtraState } from './state-extra.js';
import { ScheduleState } from './state-schedule.js';
import { SettingsState } from './state-settings.js';
import { LogsState } from './state-logs.js';
import { ManualState } from './state-manual.js';
import { ActivityState } from './state-activity.js';
import { FormatHelpers } from './format.js';

document.addEventListener('alpine:init', () => {
    Alpine.data('app', () => ({
        ...AuthState(),
        ...StatusState(),
        ...DefaultsState(),
        ...UsersState(),
        ...ExtraState(),
        ...ScheduleState(),
        ...SettingsState(),
        ...LogsState(),
        ...ManualState(),
        ...ActivityState(),
        ...FormatHelpers(),

        tab: localStorage.getItem('activeTab') || 'status',

        statusMsg: '',
        error: '',
        savingToday: false,
        savingSchedule: false,
        menuOpen: false,

        async loadData() {
            await this.loadGlobal();
            await this.loadSettings();

            // loadUsers теперь сам выберет нужного пользователя (из localStorage или первого)
            await this.loadUsers();
            await this.loadLogDates();

            // 🆕 Загружаем данные для выбранного пользователя
            if (this.selectedUser) {
                await this.loadUserData();
                this.initActivity();
            }

            this.startAutoRefresh();
        },

        showStatus(msg) {
            this.statusMsg = msg;
            setTimeout(() => this.statusMsg = '', 3000);
        },

        switchTab(newTab) {
            this.tab = newTab;
            localStorage.setItem('activeTab', newTab);

            if (newTab === 'users' && this.selectedUser) {
                this.loadUserData();
                this.initActivity();
            }
        },

        switchServer() {
            window.history.back();
        },

        goToUsersTab() {
            this.switchTab('users');
        }
    }));
});