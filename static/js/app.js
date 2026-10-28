import { AuthState } from './state-auth.js';
import { StatusState } from './state-status.js';
import { DefaultsState } from './state-defaults.js';
import { UsersState } from './state-users.js';
import { ExtraState } from './state-extra.js';
import { ScheduleState } from './state-schedule.js';
import { SettingsState } from './state-settings.js';
import { LogsState } from './state-logs.js';
import { ManualState } from './state-manual.js';
import { FormatHelpers } from './format.js';

document.addEventListener('alpine:init', () => {
    Alpine.data('app', () => ({
        // Собираем все стейты
        ...AuthState(),
        ...StatusState(),
        ...DefaultsState(),
        ...UsersState(),
        ...ExtraState(),
        ...ScheduleState(),
        ...SettingsState(),
        ...LogsState(),
        ...ManualState(),
        ...FormatHelpers(),

        // Общие UI-поля (те, что были в app.js)
        tab: 'status',
        statusMsg: '',
        error: '',
        savingToday: false,
        savingSchedule: false,
        menuOpen: false,

        showStatus(msg) {
            this.statusMsg = msg;
            setTimeout(() => this.statusMsg = '', 3000);
        },

        switchTab(newTab) {
            this.tab = newTab;
            if (newTab === 'users' && this.selectedUser) {
                this.loadUserData();
            }
        },

        switchServer() {
            window.history.back();
        }
    }));
});