import { apiGet, apiPost } from './api.js';

export const SettingsState = () => ({
    settings: [],
    savingCategory: null,

    async loadSettings() {
        try {
            this.settings = await apiGet('/api/settings');
        } catch (e) {
            console.error('Ошибка загрузки настроек:', e);
        }
    },

    groupedSettings() {
        const map = new Map();
        const titles = {
            general: '⚙️ Общие',
            security: '🔐 Безопасность',
            notifications: '🔔 Уведомления',
            timing: '⏱ Время',
            control: '🎛 Управление',
            scheduler: '🕒 Планировщик',
            logs: '📜 Логи',
        };
        const order = ['general', 'security', 'notifications', 'timing', 'control', 'scheduler', 'logs'];

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

        return Array.from(map.values()).sort((a, b) => {
            const ia = order.indexOf(a.name);
            const ib = order.indexOf(b.name);
            return (ia === -1 ? 99 : ia) - (ib === -1 ? 99 : ib);
        });
    },

    enumOptions(key) {
        const map = {
            agent_mode: [
                { value: 'local', label: 'Local — только локально' },
                { value: 'hybrid', label: 'Hybrid — локально + облако (в разработке)' },
            ],
            log_level: [
                { value: 'debug', label: 'Debug — всё' },
                { value: 'info', label: 'Info — обычные события' },
                { value: 'warn', label: 'Warn — предупреждения' },
                { value: 'error', label: 'Error — только ошибки' },
            ],
        };
        return map[key] || [];
    },

    async saveSetting(item) {
        if (item.value_type === 'password' && (!item.value || item.value.length === 0)) {
            return;
        }

        item.__saving = true;
        try {
            await apiPost('/api/set_param', { key: item.key, value: item.value });
            item.__savedAt = Date.now();
            this.showStatus('✅ Сохранено');
            if (item.value_type === 'password') {
                await this.loadSettings();
            }
        } catch (e) {
            this.showStatus('❌ ' + e.message);
        } finally {
            item.__saving = false;
        }
    },

    async saveCategory(cat) {
        if (this.savingCategory) return;
        this.savingCategory = cat.name;

        try {
            const toSave = cat.items.filter(s =>
                s.value_type !== 'password' || (s.value && s.value.length > 0)
            );

            await Promise.all(toSave.map(s =>
                apiPost('/api/set_param', { key: s.key, value: s.value })
            ));

            this.showStatus(`✅ ${cat.title} сохранено`);
            await this.loadSettings();
        } catch (e) {
            console.error('[saveCategory]', e);
            this.showStatus('❌ Ошибка сети');
        } finally {
            this.savingCategory = null;
        }
    },
});
