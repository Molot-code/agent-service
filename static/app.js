// app.js

document.addEventListener('alpine:init', () => {
    Alpine.data('app', () => ({
        // ===== СОСТОЯНИЕ АВТОРИЗАЦИИ =====
        pinSet: false,
        authenticated: false,
        pin: localStorage.getItem('agent_pin') || '',
        newPin: '',
        confirmPin: '',
        error: '',
        statusMsg: '',
        refreshInterval: null,
        loadingUserData: false,
        isTauri: typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window,
        savingCategory: null,   // какая категория сейчас сохраняется (имя) или null
        savingToday: false,
        savingSchedule: false,

        // ===== ИНТЕРФЕЙС =====
        tab: 'status',
        users: [],
        selectedUser: '',
        settings: [],   // 🆕 массив настроек из /api/settings

        // ===== ГЛОБАЛЬНЫЕ НАСТРОЙКИ =====
        global: {
            lock_message: '',
            is_locked: false,
            session_locked: false,
            mode: 'scheduled',
            time_to_event: null,
            api_token: '',
            current_user: ''
        },

        // ===== АКТИВНЫЙ ПОЛЬЗОВАТЕЛЬ (для вкладки «Статус → Управление») =====
        activeToday: {
            start_time: '00:00',
            end_time: '23:59',
            limit_min: 0,
            internet_limit_min: 0,
            extra_min: 0,
            internet_extra_min: 0,
            screen_time: 0,
            internet_time: 0
        },

        // ===== ВЫБРАННЫЙ ПОЛЬЗОВАТЕЛЬ (для вкладки «Пользователи») =====
        today: {
            start_time: '08:00',
            end_time: '20:00',
            limit_min: 120,
            internet_limit_min: 120,
            extra_min: 0,
            internet_extra_min: 0,
            screen_time: 0,
            internet_time: 0
        },

        // ===== РАСПИСАНИЕ =====
        sameSchedule: true,
        dailySchedule: {
            start_time: '08:00',
            end_time: '20:00',
            limit_min: 120,
            internet_limit_min: 120
        },
        weekDays: [
            { name: 'Понедельник', start_time: '08:00', end_time: '20:00', limit_min: 120, internet_limit_min: 120 },
            { name: 'Вторник', start_time: '08:00', end_time: '20:00', limit_min: 120, internet_limit_min: 120 },
            { name: 'Среда', start_time: '08:00', end_time: '20:00', limit_min: 120, internet_limit_min: 120 },
            { name: 'Четверг', start_time: '08:00', end_time: '20:00', limit_min: 120, internet_limit_min: 120 },
            { name: 'Пятница', start_time: '08:00', end_time: '20:00', limit_min: 120, internet_limit_min: 120 },
            { name: 'Суббота', start_time: '08:00', end_time: '20:00', limit_min: 120, internet_limit_min: 120 },
            { name: 'Воскресенье', start_time: '08:00', end_time: '20:00', limit_min: 120, internet_limit_min: 120 }
        ],

        // ===== МЕТОДЫ ИНИЦИАЛИЗАЦИИ =====

        async init() {
            try {
                const res = await fetch('/api/is_pin_set');
                const data = await res.json();
                this.pinSet = data.is_set;

                if (this.pinSet && this.pin) {
                    await this.login();
                }
            } catch (e) {
                this.error = 'Ошибка подключения к серверу';
            }
        },

        async setupPin() {
            if (this.newPin.length < 4) { this.error = 'PIN должен быть минимум 4 цифры'; return; }
            if (this.newPin !== this.confirmPin) { this.error = 'PIN-коды не совпадают'; return; }

            const res = await fetch('/api/set_pin', {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({ new_pin: this.newPin })
            });
            const data = await res.json();

            if (data.success) {
                this.pin = this.newPin;
                this.newPin = '';
                this.confirmPin = '';
                this.error = '';
                this.pinSet = true;
                this.authenticated = true;
                localStorage.setItem('agent_pin', this.pin);
                this.loadData();
            } else {
                this.error = data.message;
            }
        },

        async login() {
            if (!this.pin) { this.error = 'Введите PIN'; return; }

            const res = await fetch('/api/check_pin', {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({ new_pin: this.pin })
            });
            const data = await res.json();

            if (data.success) {
                this.error = '';
                this.authenticated = true;
                localStorage.setItem('agent_pin', this.pin);
                this.loadData();
            } else {
                this.error = data.message;
            }
        },

        logout() {
            this.authenticated = false;
            this.pin = '';
            localStorage.removeItem('agent_pin');
            this.stopAutoRefresh();
        },

        // ===== ЗАГРУЗКА ДАННЫХ =====

        async loadData() {
            await this.loadGlobal();
            await this.loadSettings();   // 🆕
            await this.loadUsers();
            this.startAutoRefresh();
        },

        startAutoRefresh() {
            this.stopAutoRefresh();

            // Только статус системы — каждые 5 сек.
            // Данные выбранного/активного пользователя НЕ обновляем автоматически,
            // чтобы не затирать несохранённые правки. Кнопка «🔄 Обновить» — вручную.
            this.refreshInterval = setInterval(async () => {
                await this.loadGlobal();
            }, 5000);

            console.log('✅ Автообновление статуса запущено (5с)');
        },

        stopAutoRefresh() {
            if (this.refreshInterval) {
                clearInterval(this.refreshInterval);
                this.refreshInterval = null;
            }
        },
        async loadGlobal() {
            try {
                const url = this.selectedUser
                    ? `/api/status?user_sid=${this.selectedUser}`
                    : '/api/status';

                const res = await fetch(url, {
                    headers: { 'Authorization': `Bearer ${this.pin}` }
                });

                if (res.ok) {
                    const data = await res.json();

                    this.global.lock_message = data.lock_message || '';
                    this.global.is_locked = data.is_locked || false;
                    this.global.session_locked = data.session_locked || false;
                    this.global.mode = data.mode || 'scheduled';
                    this.global.current_user = data.current_user || '';
                    this.global.api_token = data.api_token || '';
                    this.global.time_to_event = data.time_to_event ?? null;

                    if (data.current_user && data.current_user !== 'Нет активных сессий') {
                        await this.loadActiveToday(data.current_user);
                    }
                }
            } catch (e) {
                console.error('Ошибка загрузки статуса:', e);
            }
        },
        async loadSettings() {
            try {
                const res = await fetch('/api/settings', {
                    headers: { 'Authorization': `Bearer ${this.pin}` }
                });
                if (res.ok) {
                    this.settings = await res.json();
                }
            } catch (e) {
                console.error('Ошибка загрузки настроек:', e);
            }
        },
        async loadActiveToday(username) {
            try {
                const user = this.users.find(u => u.username === username);
                if (!user) return;

                const res = await fetch(`/api/today/${user.sid}`, {
                    headers: { 'Authorization': `Bearer ${this.pin}` }
                });
                if (!res.ok) return;

                const data = await res.json();
                this.activeToday = {
                    start_time: data.start_time || '00:00',
                    end_time: data.end_time || '23:59',
                    limit_min: Math.floor((data.limit_sec ?? 0) / 60),
                    internet_limit_min: Math.floor((data.internet_limit_sec ?? 0) / 60),
                    // ⚠️ на бэке поля называются extra_time_min / extra_internet_time_min
                    extra_min: data.extra_time_min || 0,
                    internet_extra_min: data.extra_internet_time_min || 0,
                    screen_time: data.current_screen_time || 0,
                    internet_time: data.current_internet_time || 0
                };
            } catch (e) {
                console.error('Ошибка загрузки today активного:', e);
            }
        },

        async loadUsers() {
            try {
                const res = await fetch('/api/users', {
                    headers: { 'Authorization': `Bearer ${this.pin}` }
                });
                if (!res.ok) return;

                this.users = await res.json();

                if (this.users.length > 0 && !this.selectedUser) {
                    // Приоритет — активный пользователь
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
                // ===== СЕГОДНЯ (форма + счётчики) =====
                const res = await fetch(`/api/today/${this.selectedUser}`, {
                    headers: { 'Authorization': `Bearer ${this.pin}` }
                });
                if (res.ok) {
                    const data = await res.json();
                    this.today = {
                        start_time: data.start_time || '08:00',
                        end_time: data.end_time || '20:00',
                        limit_min: Math.floor((data.limit_sec ?? 7200) / 60),               // ← ?? вместо ||
                        internet_limit_min: Math.floor((data.internet_limit_sec ?? 7200) / 60),  // ← ??
                        extra_min: data.extra_time_min || 0,
                        internet_extra_min: data.extra_internet_time_min || 0,
                        screen_time: data.current_screen_time || 0,
                        internet_time: data.current_internet_time || 0
                    };
                }

                // ===== РАСПИСАНИЕ НА НЕДЕЛЮ =====
                const schedRes = await fetch(`/api/schedule/${this.selectedUser}`, {
                    headers: { 'Authorization': `Bearer ${this.pin}` }
                });
                if (schedRes.ok) {
                    const { schedule } = await schedRes.json();
                    if (schedule?.length === 7) {
                        // ⚠️ На фронте weekDays идёт Пн..Вс.
                        // На бэке day_of_week: 0 = вс, 1 = пн, ..., 6 = сб.
                        // Поэтому сопоставляем по дню недели, а не по индексу массива.
                        schedule.forEach((day) => {
                            const frontendIdx = day.day_of_week === 0 ? 6 : day.day_of_week - 1;
                            const d = this.weekDays[frontendIdx];
                            if (!d) return;
                            d.start_time = day.start_time;
                            d.end_time = day.end_time;
                            d.limit_min = Math.floor(day.limit_sec / 60);
                            d.internet_limit_min = Math.floor(day.internet_limit_sec / 60);
                        });
                        this.sameSchedule = this.checkSameSchedule();
                        this.syncDailyFromWeek();
                    }
                }
            } catch (e) {
                console.error('Ошибка загрузки данных пользователя:', e);
            } finally {
                this.loadingUserData = false;
            }
        },

        // ===== УПРАВЛЕНИЕ ПОЛЬЗОВАТЕЛЕМ =====

        async toggleControl(sid, enabled) {
            if (!sid) return;

            try {
                const res = await fetch('/api/users/toggle_control', {
                    method: 'POST',
                    headers: {
                        'Content-Type': 'application/json',
                        'Authorization': `Bearer ${this.pin}`
                    },
                    body: JSON.stringify({ sid, enabled })
                });

                if (res.ok) {
                    const user = this.users.find(u => u.sid === sid);
                    if (user) user.is_control_enabled = enabled;
                    console.log(`✅ Контроль для ${sid}: ${enabled ? 'ВКЛ' : 'ВЫКЛ'}`);
                    // Синк делает бэкенд сам — здесь ничего не дёргаем
                    await this.loadGlobal();
                } else {
                    this.showStatus('❌ Ошибка сохранения');
                }
            } catch (e) {
                console.error('Ошибка сети:', e);
                this.showStatus('❌ Ошибка сети');
            }
        },

        goToUsersTab() {
            this.tab = 'users';

            if (this.global.current_user && this.global.current_user !== 'Нет активных сессий') {
                const user = this.users.find(u => u.username === this.global.current_user);
                if (user) {
                    this.selectedUser = user.sid;
                    this.loadUserData();
                }
            }
        },

        async enableControlForSelectedUser() {
            if (!this.selectedUser) return;
            await this.toggleControl(this.selectedUser, true);
        },

        // ===== УПРАВЛЕНИЕ КОМАНДАМИ =====

        // ===== СОХРАНЕНИЕ НАСТРОЕК =====

        async saveGlobal() {
            try {
                await fetch('/api/set_param', {
                    method: 'POST',
                    headers: { 'Content-Type': 'application/json', 'Authorization': `Bearer ${this.pin}` },
                    body: JSON.stringify({ key: 'lock_message', value: this.global.lock_message })
                });
                this.showStatus('✅ Глобальные настройки сохранены');
                await this.loadGlobal();
            } catch (e) {
                this.showStatus('❌ Ошибка сохранения');
            }
        },

        async saveToday() {
            if (this.savingToday) return;
            this.savingToday = true;
            try {
                const res = await fetch(`/api/today/${this.selectedUser}`, {
                    method: 'POST',
                    headers: { 'Content-Type': 'application/json', 'Authorization': `Bearer ${this.pin}` },
                    body: JSON.stringify({
                        limit_sec: this.today.limit_min * 60,
                        internet_limit_sec: this.today.internet_limit_min * 60,
                        extra_time_min: this.today.extra_min,
                        extra_internet_time_min: this.today.internet_extra_min,
                        start_time: this.today.start_time,
                        end_time: this.today.end_time
                    })
                });
                if (res.ok) {
                    this.showStatus('✅ Лимит на сегодня сохранён');
                    await this.loadUserData();
                } else {
                    this.showStatus('❌ Ошибка сохранения');
                }
            } catch (e) {
                this.showStatus('❌ Ошибка сохранения');
            } finally {
                this.savingToday = false;
            }
        },

        async saveWeekSchedule() {
            if (this.savingSchedule) return;
            this.savingSchedule = true;
            try {
                if (this.sameSchedule) {
                    this.applyDailyToAll();
                }
                const schedule = this.weekDays.map((day, idx) => ({
                    day_of_week: idx === 6 ? 0 : idx + 1,
                    limit_sec: day.limit_min * 60,
                    internet_limit_sec: day.internet_limit_min * 60,
                    start_time: day.start_time,
                    end_time: day.end_time
                }));
                const res = await fetch(`/api/schedule/${this.selectedUser}`, {
                    method: 'POST',
                    headers: { 'Content-Type': 'application/json', 'Authorization': `Bearer ${this.pin}` },
                    body: JSON.stringify({ schedule })
                });
                if (res.ok) {
                    this.showStatus('✅ Расписание сохранено');
                    await this.loadUserData();
                } else {
                    this.showStatus('❌ Ошибка сохранения');
                }
            } catch (e) {
                this.showStatus('❌ Ошибка сохранения');
            } finally {
                this.savingSchedule = false;
            }
        },

        // ===== EXTRA TIME для активного пользователя =====

        async addExtraTime(key, minutes, opts = {}) {
            if (!key || minutes === null || minutes === undefined || minutes === 0) return;

            const user = this.users.find(u => u.username === this.global.current_user);
            if (!user) {
                this.showStatus('❌ Активный пользователь не найден');
                return;
            }

            const body = { sid: user.sid };
            if (key === 'screen') body.screen_minutes = minutes;
            if (key === 'internet') body.internet_minutes = minutes;

            try {
                const res = await fetch('/api/extra_time', {
                    method: 'POST',
                    headers: {
                        'Content-Type': 'application/json',
                        'Authorization': `Bearer ${this.pin}`
                    },
                    body: JSON.stringify(body)
                });

                if (res.ok) {
                    // Локально значение уже обновлено (previewExtra),
                    // здесь только подтверждаем и тихо тянем статус.
                    if (!opts.silent) {
                        if (key === 'screen') this.activeToday.extra_min += minutes;
                        if (key === 'internet') this.activeToday.internet_extra_min += minutes;
                    }
                    if (opts.silent) this.syncExtraSnapshot();
                } else {
                    this.showStatus('❌ Ошибка сохранения');
                }
            } catch (e) {
                this.showStatus('❌ Ошибка сети');
            }
        },

        async resetExtraTime(kind) {
            if (kind === 'screen') {
                this.previewExtra('screen', 0);
                await this.commitExtra('screen');
            } else if (kind === 'internet') {
                this.previewExtra('internet', 0);
                await this.commitExtra('internet');
            }
        },
        // ===== EXTRA TIME: СЛАЙДЕР С ОТЛОЖЕННЫМ СОХРАНЕНИЕМ =====

        // Запоминаем «серверное» значение, чтобы знать, что реально менять
        extraSnapshot: {
            screen: 0,
            internet: 0,
        },

        // Копируем состояние с сервера в snapshot (вызывается после loadGlobal)
        syncExtraSnapshot() {
            this.extraSnapshot.screen = this.activeToday.extra_min || 0;
            this.extraSnapshot.internet = this.activeToday.internet_extra_min || 0;
        },

        // Пользователь двигает ползунок — просто обновляем локальное значение,
        // никаких запросов. UI перерисовывается мгновенно.
        previewExtra(kind, value) {
            const v = Number(value) || 0;
            if (kind === 'screen') this.activeToday.extra_min = v;
            else this.activeToday.internet_extra_min = v;
        },

        // Пользователь отпустил ползунок / ушёл с поля — сохраняем дельту
        async commitExtra(kind) {
            const current = kind === 'screen'
                ? this.activeToday.extra_min
                : this.activeToday.internet_extra_min;
            const previous = this.extraSnapshot[kind] || 0;

            const delta = current - previous;
            if (delta === 0) return;   // нечего сохранять

            await this.addExtraTime(kind, delta, { silent: true });

            // Обновляем snapshot по факту успеха
            this.extraSnapshot[kind] = current;
        },

        // Кнопки «−» / «+» — двигаем и сразу сохраняем
        async nudgeExtra(kind, step) {
            const current = kind === 'screen'
                ? this.activeToday.extra_min
                : this.activeToday.internet_extra_min;
            const next = Math.max(-60, Math.min(60, current + step));
            this.previewExtra(kind, next);
            await this.commitExtra(kind);
        },
        // ===== PIN =====

        async changePin() {
            if (this.newPin.length < 4) {
                this.showStatus('❌ PIN должен быть минимум 4 цифры');
                return;
            }

            const res = await fetch('/api/set_pin', {
                method: 'POST',
                headers: { 'Content-Type': 'application/json', 'Authorization': `Bearer ${this.pin}` },
                body: JSON.stringify({ old_pin: this.pin, new_pin: this.newPin })
            });
            const data = await res.json();

            if (data.success) {
                this.pin = this.newPin;
                this.newPin = '';
                localStorage.setItem('agent_pin', this.pin);
                this.showStatus('✅ PIN изменён');
            } else {
                this.showStatus('❌ ' + data.message);
            }
        },

        async switchTab(newTab) {
            this.tab = newTab;

            // При переходе на «Пользователи» — перезагружаем данные выбранного пользователя.
            // Это нужно, потому что за время на других вкладках:
            //   - лимиты могли измениться (например, из /admin_action),
            //   - extra_min подтянут из другого места,
            //   - расписание поменялось,
            //   - дата перешла на новый день.
            if (newTab === 'users' && this.selectedUser) {
                await this.loadUserData();
            }
        },

        // ===== ВСПОМОГАТЕЛЬНЫЕ =====

        // Эффективный лимит активного пользователя (базовый + extra), в секундах
        effectiveLimitSec(kind) {
            if (kind === 'screen') {
                return (this.activeToday.limit_min + this.activeToday.extra_min) * 60;
            }
            if (kind === 'internet') {
                return (this.activeToday.internet_limit_min + this.activeToday.internet_extra_min) * 60;
            }
            return 0;
        },

        checkSameSchedule() {
            if (this.weekDays.length < 7) return false;
            const first = this.weekDays[0];
            return this.weekDays.every(d =>
                d.start_time === first.start_time &&
                d.end_time === first.end_time &&
                d.limit_min === first.limit_min &&
                d.internet_limit_min === first.internet_limit_min
            );
        },

        syncDailyFromWeek() {
            if (this.weekDays.length === 0) return;
            const d = this.weekDays[0];
            this.dailySchedule = {
                start_time: d.start_time,
                end_time: d.end_time,
                limit_min: d.limit_min,
                internet_limit_min: d.internet_limit_min
            };
        },

        applyDailyToAll() {
            const d = this.dailySchedule;
            this.weekDays.forEach(day => {
                day.start_time = d.start_time;
                day.end_time = d.end_time;
                day.limit_min = d.limit_min;
                day.internet_limit_min = d.internet_limit_min;
            });
        },

        onSameScheduleToggle() {
            if (this.sameSchedule) {
                this.syncDailyFromWeek();
                this.applyDailyToAll();
            }
        },

        formatMinutes(seconds) {
            if (!seconds || seconds < 0) seconds = 0;
            const m = Math.floor(seconds / 60);
            const h = Math.floor(m / 60);
            const remM = m % 60;
            return h > 0 ? `${h}ч ${remM}м` : `${m} мин`;
        },

        showStatus(msg) {
            this.statusMsg = msg;
            setTimeout(() => this.statusMsg = '', 3000);
        },

        formatTimeToEvent(ev) {
            if (!ev) return '';
            const { kind, minutes } = ev;
            if (minutes === null || minutes === undefined) return '';
            const m = minutes;
            const h = Math.floor(m / 60);
            const rem = m % 60;
            const timeStr = h > 0 && rem > 0 ? `${h} ч ${rem} мин`
                : h > 0 ? `${h} ч`
                    : `${m} мин`;

            if (kind === 'lock_in') return `До блокировки: ${timeStr}`;
            if (kind === 'unlock_in') return `До разблокировки: ${timeStr}`;
            return timeStr;
        },

        progressPercent(used, limit) {
            if (!limit || limit <= 0) return 0;
            return Math.min(100, Math.round((used / limit) * 100));
        },

        progressColor(percent) {
            if (percent >= 90) return 'bg-red-500';
            if (percent >= 70) return 'bg-yellow-500';
            return 'bg-green-500';
        },

        extraColor(value) {
            if (value > 0) return 'text-green-600';
            if (value < 0) return 'text-red-600';
            return 'text-gray-500';
        },

        extraText(value) {
            return (value > 0 ? '+' : '') + value + ' мин';
        },
        modeLabel() {
            if (this.global.mode === 'no_user') return '💤 Нет сессий';
            if (this.global.mode === 'control_disabled') return '🚫 Контроль выключен';
            if (this.global.mode === 'scheduled') return '🤖 По расписанию';
            if (this.global.mode === 'manual_lock') return '🔒 Ручная блокировка';
            if (this.global.mode === 'manual_unlock') return '🔓 Ручная разблокировка';
            return '👤 Ручной';
        },

        modeColor() {
            if (this.global.mode === 'control_disabled') return 'text-gray-600';
            if (this.global.mode === 'scheduled') return 'text-green-600';
            if (this.global.mode === 'manual_lock') return 'text-red-600';
            if (this.global.mode === 'manual_unlock') return 'text-yellow-600';
            return 'text-blue-600';
        },

        stateLabel() {
            if (this.global.mode === 'no_user') return 'НЕТ АКТИВНЫХ СЕССИЙ';
            if (this.global.mode === 'control_disabled') return 'АКТИВНО';
            if (this.global.is_locked) return 'ЗАБЛОКИРОВАНО';
            if (this.global.mode === 'manual_lock') return 'Заблокировано вручную';
            if (this.global.mode === 'manual_unlock') return 'Разблокировано вручную';
            return 'АКТИВНО';
        },

        stateColor() {
            if (this.global.mode === 'no_user') return 'text-gray-400';
            if (this.global.mode === 'control_disabled') return 'text-green-600';
            if (this.global.is_locked) return 'text-red-600';
            if (this.global.mode === 'manual_lock') return 'text-red-600';
            if (this.global.mode === 'manual_unlock') return 'text-yellow-600';
            return 'text-green-600';
        },

        formatTimeUntil(minutes) {
            if (minutes === null || minutes <= 0) return '';
            const h = Math.floor(minutes / 60);
            const m = minutes % 60;
            if (h > 0 && m > 0) return `${h} ч ${m} мин`;
            if (h > 0) return `${h} ч`;
            return `${m} мин`;
        },

        menuOpen: false,

        switchServer() {
            window.history.back();
        },
        // ===== МЕНЮ БЛОКИРОВКИ =====
        showLockMenu: false,

        // Хелпер: "YYYY-MM-DD HH:MM:SS" через N минут от сейчас
        formatDateTimeFromOffset(minutes) {
            const d = new Date(Date.now() + minutes * 60 * 1000);
            const pad = (n) => String(n).padStart(2, '0');
            return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ` +
                `${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`;
        },

        // Универсальный вызов manual_action
        async manualAction(action, until) {
            this.showLockMenu = false;
            try {
                const res = await fetch('/api/manual_action', {
                    method: 'POST',
                    headers: {
                        'Content-Type': 'application/json',
                        'Authorization': `Bearer ${this.pin}`
                    },
                    body: JSON.stringify({ action, until })
                });
                const data = await res.json();
                this.showStatus(data.message || '✅ Команда принята');
                await this.loadGlobal();
            } catch (e) {
                this.showStatus('❌ Ошибка сети');
            }
        },

        // Обёртки для Lock
        lockUntilEndOfDay() { this.manualAction('lock', '-1'); },
        lockForMinutes(m) { this.manualAction('lock', this.formatDateTimeFromOffset(m)); },

        // Обёртки для Unlock
        unlockUntilEndOfDay() { this.manualAction('unlock', '-1'); },
        unlockForMinutes(m) { this.manualAction('unlock', this.formatDateTimeFromOffset(m)); },


        groupedSettings() {
            const map = new Map();
            const titles = {
                security: '🔐 Безопасность',
                notifications: '🔔 Уведомления',
                timing: '⏱ Время',
                general: '⚙️ Общие',
            };

            this.settings.forEach(s => {
                if (s.category === 'internal') return;
                if (!map.has(s.category)) {
                    map.set(s.category, {
                        name: s.category,
                        title: titles[s.category] || s.category,
                        items: [],
                    });
                }
                map.get(s.category).items.push(s);
            });

            return Array.from(map.values());
        },

        enumOptions(key) {
            const map = {
                agent_mode: [
                    { value: 'local', label: 'Local — только локально' },
                    { value: 'hybrid', label: 'Hybrid — локально + облако' },
                    // 'cloud' намеренно не показываем
                ],
            };
            return map[key] || [];
        },
        async saveCategory(cat) {
            if (this.savingCategory) return;
            this.savingCategory = cat.name;

            try {
                const toSave = cat.items.filter(s =>
                    s.value_type !== 'password' || (s.value && s.value.length > 0)
                );

                const promises = toSave.map(s => fetch('/api/set_param', {
                    method: 'POST',
                    headers: {
                        'Content-Type': 'application/json',
                        'Authorization': `Bearer ${this.pin}`
                    },
                    body: JSON.stringify({ key: s.key, value: s.value })
                }));

                const results = await Promise.all(promises);
                const allOk = results.every(r => r.ok);

                if (allOk) {
                    this.showStatus(`✅ ${cat.title} сохранено`);
                    await this.loadSettings();
                } else {
                    this.showStatus('❌ Ошибка сохранения');
                }
            } catch (e) {
                console.error('[saveCategory]', e);
                this.showStatus('❌ Ошибка сети');
            } finally {
                this.savingCategory = null;   // 🆕 ВОТ ЭТО ДОБАВИЛИ
            }
        },
        reasonText() {
            const ev = this.global.time_to_event;
            if (!ev || !ev.reason) return '';

            const map = {
                limit: 'из-за исчерпания лимита экранного времени',
                schedule: 'по расписанию',
                manual: 'вручную (по таймеру родителя)',
            };
            return map[ev.reason] || '';
        },
        eventTitle() {
            const ev = this.global.time_to_event;
            if (!ev) return '';

            const action = ev.kind === 'unlock_in' ? '🔓 Разблокируется' : '🔒 Заблокируется';

            const reasonMap = {
                limit: 'Лимит экранного времени',
                schedule: 'Расписание',
                manual: 'Ручной режим',
            };
            const reason = reasonMap[ev.reason] || '';

            return reason ? `${action} (${reason})` : action;
        },

        clampInternetLimit() {
            if (this.today.internet_limit_min > this.today.limit_min) {
                this.today.internet_limit_min = this.today.limit_min;
            }
            if (this.today.internet_limit_min < 0) {
                this.today.internet_limit_min = 0;
            }
        },

        clampInternetLimitForDay(idx) {
            const day = this.weekDays[idx];
            if (!day) return;
            if (day.internet_limit_min > day.limit_min) {
                day.internet_limit_min = day.limit_min;
            }
            if (day.internet_limit_min < 0) {
                day.internet_limit_min = 0;
            }
        },

        clampDailyInternetLimit() {
            if (this.dailySchedule.internet_limit_min > this.dailySchedule.limit_min) {
                this.dailySchedule.internet_limit_min = this.dailySchedule.limit_min;
            }
            if (this.dailySchedule.internet_limit_min < 0) {
                this.dailySchedule.internet_limit_min = 0;
            }
        },

        logsDate: '',
        logsSource: 'service',
        logsLevel: 'info',
        logsDates: [],
        logsContent: '',

        async loadLogDates() {
            const res = await fetch('/api/logs', {
                headers: { 'Authorization': `Bearer ${this.pin}` }
            });
            if (res.ok) {
                this.logsDates = await res.json();
                if (this.logsDates.length > 0 && !this.logsDate) {
                    this.logsDate = this.logsDates[0];
                }
            }
        },

        async loadLogs() {
            if (!this.logsDate) return;
            const url = `/api/logs/${this.logsDate}?source=${this.logsSource}&level=${this.logsLevel}&limit=1000`;
            const res = await fetch(url, {
                headers: { 'Authorization': `Bearer ${this.pin}` }
            });
            if (res.ok) {
                const data = await res.json();
                this.logsContent = data.lines.join('\n') || '(пусто)';
            }
        },

        // ===== АВТОСОХРАНЕНИЕ: ПОЛЕ ИЗМЕНИЛОСЬ =====

        // --- Глобальные настройки: сохранить одно поле ---
        async saveSetting(item) {
            // пароль сохраняем только если реально введён
            if (item.value_type === 'password' && (!item.value || item.value.length === 0)) {
                return;
            }

            item.__saving = true;
            try {
                const res = await fetch('/api/set_param', {
                    method: 'POST',
                    headers: {
                        'Content-Type': 'application/json',
                        'Authorization': `Bearer ${this.pin}`
                    },
                    body: JSON.stringify({ key: item.key, value: item.value })
                });

                if (res.ok) {
                    item.__savedAt = Date.now();
                    this.showStatus('✅ Сохранено');
                    // для пароля — очистить поле и перечитать настройки
                    if (item.value_type === 'password') {
                        await this.loadSettings();
                    }
                } else {
                    this.showStatus('❌ Ошибка сохранения');
                }
            } catch (e) {
                this.showStatus('❌ Ошибка сети');
            } finally {
                item.__saving = false;
            }
        },

        // --- Сегодня: автосохранение ---
        async saveToday() {
            if (this.savingToday) return;
            this.savingToday = true;
            try {
                const res = await fetch(`/api/today/${this.selectedUser}`, {
                    method: 'POST',
                    headers: {
                        'Content-Type': 'application/json',
                        'Authorization': `Bearer ${this.pin}`
                    },
                    body: JSON.stringify({
                        limit_sec: this.today.limit_min * 60,
                        internet_limit_sec: this.today.internet_limit_min * 60,
                        extra_time_min: this.today.extra_min,
                        extra_internet_time_min: this.today.internet_extra_min,
                        start_time: this.today.start_time,
                        end_time: this.today.end_time
                    })
                });
                if (res.ok) {
                    this.showStatus('✅ Сохранено');
                    // Не перечитываем loadUserData здесь —
                    // это собьёт фокус и «прыгнет» значениям под курсором.
                } else {
                    this.showStatus('❌ Ошибка сохранения');
                }
            } catch (e) {
                this.showStatus('❌ Ошибка сети');
            } finally {
                this.savingToday = false;
            }
        },

        async saveWeekSchedule() {
            if (this.savingSchedule) return;
            this.savingSchedule = true;
            try {
                if (this.sameSchedule) this.applyDailyToAll();
                const schedule = this.weekDays.map((day, idx) => ({
                    day_of_week: idx === 6 ? 0 : idx + 1,
                    limit_sec: day.limit_min * 60,
                    internet_limit_sec: day.internet_limit_min * 60,
                    start_time: day.start_time,
                    end_time: day.end_time
                }));
                const res = await fetch(`/api/schedule/${this.selectedUser}`, {
                    method: 'POST',
                    headers: {
                        'Content-Type': 'application/json',
                        'Authorization': `Bearer ${this.pin}`
                    },
                    body: JSON.stringify({ schedule })
                });
                if (res.ok) {
                    this.showStatus('✅ Расписание сохранено');
                } else {
                    this.showStatus('❌ Ошибка сохранения');
                }
            } catch (e) {
                this.showStatus('❌ Ошибка сети');
            } finally {
                this.savingSchedule = false;
            }
        },
    }));
});