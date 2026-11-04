// web/src/app.js
// Точка входа esbuild. Единственное место, где перечислены все стейты.
// Сборка: task js → web/static/app.min.js
// Новый state-*.js добавлять сюда (named import + ...State() в мерж).

import './api.js';   // side-effect: гарантирует загрузку apiGet/apiPost до стейтов

import { FormatHelpers } from './format.js';

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
import { RefreshState } from './state-refresh.js';

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
        ...RefreshState(),
        ...FormatHelpers(),

        tab: localStorage.getItem('activeTab') || 'status',

        statusMsg: '',
        error: '',
        savingToday: false,
        savingSchedule: false,
        menuOpen: false,

        async loadData() {
            await this.loadSettings();
            await this.loadUsers();

            await this.loadGlobal();
            await this.loadLogDates();

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