import { apiGet, apiPost } from './api.js';

export const AuthState = () => ({
    pinSet: false,
    authenticated: false,
    pin: localStorage.getItem('agent_pin') || '',
    newPin: '',
    confirmPin: '',

    async init() {
        try {
            const data = await apiGet('/api/is_pin_set');
            this.pinSet = data.is_set;
            if (this.pinSet && this.pin) await this.login();
        } catch (e) {
            this.error = 'Ошибка подключения к серверу';
        }
    },

    async setupPin() {
        if (this.newPin.length < 4) { this.error = 'PIN должен быть минимум 4 цифры'; return; }
        if (this.newPin !== this.confirmPin) { this.error = 'PIN-коды не совпадают'; return; }

        try {
            const data = await apiPost('/api/set_pin', { new_pin: this.newPin });
            if (!data.success) { this.error = data.message; return; }
            this.pin = this.newPin;
            this.newPin = '';
            this.confirmPin = '';
            this.error = '';
            this.pinSet = true;
            this.authenticated = true;
            localStorage.setItem('agent_pin', this.pin);
            this.loadData();
        } catch (e) {
            this.error = e.message;
        }
    },

    async login() {
        if (!this.pin) { this.error = 'Введите PIN'; return; }
        try {
            // ⚠️ ФИКС: шлём { pin }, а не { new_pin } — так хочет CheckPinRequest
            const data = await apiPost('/api/check_pin', { pin: this.pin });
            if (data.success) {
                this.error = '';
                this.authenticated = true;
                localStorage.setItem('agent_pin', this.pin);
                this.loadData();
            } else {
                this.error = data.message;
            }
        } catch (e) {
            this.error = e.message;
        }
    },

    logout() {
        this.authenticated = false;
        this.pin = '';
        localStorage.removeItem('agent_pin');
        this.stopAutoRefresh();
    },

    async changePin() {
        if (this.newPin.length < 4) {
            this.showStatus('❌ PIN должен быть минимум 4 цифры');
            return;
        }
        try {
            const data = await apiPost('/api/set_pin', {
                old_pin: this.pin,
                new_pin: this.newPin,
            });
            if (data.success) {
                this.pin = this.newPin;
                this.newPin = '';
                localStorage.setItem('agent_pin', this.pin);
                this.showStatus('✅ PIN изменён');
            } else {
                this.showStatus('❌ ' + data.message);
            }
        } catch (e) {
            this.showStatus('❌ ' + e.message);
        }
    },
});