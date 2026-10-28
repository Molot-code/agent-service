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