export const FormatHelpers = () => ({
    formatMinutes(seconds) {
        if (!seconds || seconds < 0) seconds = 0;
        const m = Math.floor(seconds / 60);
        const h = Math.floor(m / 60);
        const rem = m % 60;
        return h > 0 ? `${h}ч ${rem}м` : `${m} мин`;
    },

    formatTimeToEvent(ev) {
        if (!ev) return '';
        const { kind, minutes } = ev;
        if (minutes == null) return '';
        const h = Math.floor(minutes / 60);
        const rem = minutes % 60;
        const s = h > 0 && rem > 0 ? `${h} ч ${rem} мин`
            : h > 0 ? `${h} ч` : `${minutes} мин`;
        if (kind === 'lock_in') return `До блокировки: ${s}`;
        if (kind === 'unlock_in') return `До разблокировки: ${s}`;
        return s;
    },

    progressPercent(used, limit) {
        if (!limit || limit <= 0) return 0;
        return Math.min(100, Math.round((used / limit) * 100));
    },

    progressColor(p) {
        if (p >= 90) return 'bg-red-500';
        if (p >= 70) return 'bg-yellow-500';
        return 'bg-green-500';
    },

    extraColor(v) { return v > 0 ? 'text-green-600' : v < 0 ? 'text-red-600' : 'text-gray-500'; },
    extraText(v) { return (v > 0 ? '+' : '') + v + ' мин'; },

    modeLabel() {
        const map = {
            no_user: '💤 Нет сессий',
            control_disabled: '🚫 Контроль выключен',
            scheduled: '🤖 По расписанию',
            manual_lock: '🔒 Ручная блокировка',
            manual_unlock: '🔓 Ручная разблокировка',
        };
        return map[this.global.mode] || '👤 Ручной';
    },

    modeColor() {
        const map = {
            control_disabled: 'text-gray-600',
            scheduled: 'text-green-600',
            manual_lock: 'text-red-600',
            manual_unlock: 'text-yellow-600',
        };
        return map[this.global.mode] || 'text-blue-600';
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

    reasonText() {
        const ev = this.global.time_to_event;
        if (!ev?.reason) return '';
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
        const reasonMap = { limit: 'Лимит экранного времени', schedule: 'Расписание', manual: 'Ручной режим' };
        const reason = reasonMap[ev.reason] || '';
        return reason ? `${action} (${reason})` : action;
    },

    formatTimeUntil(minutes) {
        if (minutes === null || minutes <= 0) return '';
        const h = Math.floor(minutes / 60);
        const m = minutes % 60;
        if (h > 0 && m > 0) return `${h} ч ${m} мин`;
        if (h > 0) return `${h} ч`;
        return `${m} мин`;
    },
    getWeekDayName(dow) {
        const names = ['Воскресенье', 'Понедельник', 'Вторник', 'Среда', 'Четверг', 'Пятница', 'Суббота'];
        return names[dow] || 'День';
    },
    formatDuration(seconds) {
        if (!seconds) return '0 мин';
        const h = Math.floor(seconds / 3600);
        const m = Math.floor((seconds % 3600) / 60);
        return h > 0 ? `${h} ч ${m} мин` : `${m} мин`;
    },
});