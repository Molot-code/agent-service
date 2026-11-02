import { apiGet } from './api.js';

// 🔥 Хелпер для получения локальной даты в формате YYYY-MM-DD
function getLocalToday() {
    const d = new Date();
    return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`;
}

export const ActivityState = () => ({
    activityLog: [],
    activityLoading: false,
    showEvents: false,
    // ===== Выбранная дата для графика (YYYY-MM-DD) =====
    activityDate: getLocalToday(),

    // ===== Месяц/год для навигации в date picker =====
    activityPickerMonth: new Date().getMonth(),
    activityPickerYear: new Date().getFullYear(),

    // ===== Загрузка активности за выбранную дату =====
    async loadActivityLog() {
        if (!this.selectedUser) return;
        this.activityLoading = true;
        try {
            const res = await apiGet(
                `/api/activity_log/${this.selectedUser}?date=${this.activityDate}`
            );
            this.activityLog = res.entries || [];
        } catch (e) {
            console.error('Failed to load activity log:', e);
        } finally {
            this.activityLoading = false;
        }
    },

    // 🆕 Вызывается при инициализации, когда пользователь загружен
    initActivity() {
        if (this.selectedUser && this.activityLog.length === 0) {
            this.loadActivityLog();
        }
    },

    // ===== Выбор даты в picker =====
    selectActivityDate(date) {
        this.activityDate = date;
        this.loadActivityLog();
    },

    // ===== Метка даты для заголовка =====
    // Для заголовка "Активность за <...>" (оставляем слово "сегодня")
    activityDateHeader() {
        const today = getLocalToday();
        if (this.activityDate === today) return 'сегодня';
        const d = new Date(this.activityDate + 'T00:00:00');
        return d.toLocaleDateString('ru-RU', { day: 'numeric', month: 'long' });
    },

    // Для кнопки календаря (всегда показываем дату: 29.10.2026)
    activityDateLabel() {
        const d = new Date(this.activityDate + 'T00:00:00');
        return d.toLocaleDateString('ru-RU', { day: '2-digit', month: '2-digit', year: 'numeric' });
    },

    // ===== Навигация по месяцам в picker =====
    activityMonthLabel() {
        const months = ['Январь', 'Февраль', 'Март', 'Апрель', 'Май', 'Июнь',
            'Июль', 'Август', 'Сентябрь', 'Октябрь', 'Ноябрь', 'Декабрь'];
        return `${months[this.activityPickerMonth]} ${this.activityPickerYear}`;
    },

    activityPrevMonth() {
        if (this.activityPickerMonth === 0) {
            this.activityPickerMonth = 11;
            this.activityPickerYear--;
        } else {
            this.activityPickerMonth--;
        }
    },

    activityNextMonth() {
        if (this.activityPickerMonth === 11) {
            this.activityPickerMonth = 0;
            this.activityPickerYear++;
        } else {
            this.activityPickerMonth++;
        }
    },

    // ===== Сетка календаря для picker =====
    get activityCalendarRows() {
        const rows = [];
        const firstDay = new Date(this.activityPickerYear, this.activityPickerMonth, 1);
        const lastDay = new Date(this.activityPickerYear, this.activityPickerMonth + 1, 0);

        let startDay = firstDay.getDay() - 1;
        if (startDay < 0) startDay = 6;

        let day = 1;
        let week = [];

        for (let i = 0; i < startDay; i++) week.push(null);

        while (day <= lastDay.getDate()) {
            const dateStr = `${this.activityPickerYear}-${String(this.activityPickerMonth + 1).padStart(2, '0')}-${String(day).padStart(2, '0')}`;
            week.push({ day, date: dateStr });
            day++;
            if (week.length === 7) {
                rows.push(week);
                week = [];
            }
        }

        while (week.length < 7) week.push(null);
        if (week.length) rows.push(week);

        return rows;
    },

    activityDayClass(day) {
        const today = getLocalToday();
        if (day.date === this.activityDate) return 'bg-blue-500 text-white hover:bg-blue-600';
        if (day.date === today) return 'bg-blue-100 text-blue-800 hover:bg-blue-200';
        return 'text-gray-700 hover:bg-gray-100';
    },

    // ===== Отрисовка графика (слоты по 5 минут) =====
    getSlotColor(slotIndex) {
        const segments = this.buildActivitySegments();
        // 5 минут = 300 секунд
        const slotStart = slotIndex * 300;
        const slotEnd = (slotIndex + 1) * 300;

        for (const seg of segments) {
            // Если сегмент перекрывает хотя бы часть этого 5-минутного слота
            if (seg.start < slotEnd && seg.end > slotStart) {
                return this._getCssClass(seg.state);
            }
        }
        return 'bg-gray-200'; // Нет данных
    },

    _getCssClass(state) {
        if (state === 'screen_locked') return 'bg-red-500';
        if (state === 'session_locked') return 'bg-gray-600';
        if (state === 'no_internet') return 'bg-lime-300';
        if (state === 'active_scheduled') return 'bg-green-500';
        if (state === 'active_manual') return 'bg-blue-500';
        return 'bg-gray-200';
    },

    getSlotDetails(slotIndex) {
        const segments = this.buildActivitySegments();
        const slotStart = slotIndex * 300;
        const slotEnd = (slotIndex + 1) * 300;

        const startH = Math.floor(slotStart / 3600);
        const startM = Math.floor((slotStart % 3600) / 60);
        const endH = Math.floor(slotEnd / 3600);
        const endM = Math.floor((slotEnd % 3600) / 60);

        let details = `Время: ${String(startH).padStart(2, '0')}:${String(startM).padStart(2, '0')} - ${String(endH).padStart(2, '0')}:${String(endM).padStart(2, '0')}\n`;
        let found = false;

        for (const seg of segments) {
            if (seg.start < slotEnd && seg.end > slotStart) {
                const overlapStart = Math.max(seg.start, slotStart);
                const overlapEnd = Math.min(seg.end, slotEnd);
                const overlapMin = Math.floor((overlapEnd - overlapStart) / 60);

                if (overlapMin > 0) {
                    const typeLabel = this._getStateLabel(seg.state);
                    details += `${typeLabel}: ${overlapMin} мин\n`;
                    found = true;
                }
            }
        }

        if (!found) details += 'Нет данных';
        return details;
    },

    _getStateLabel(state) {
        if (state === 'screen_locked') return '🔴 Заблокирован';
        if (state === 'session_locked') return '⚫ Win+L';
        if (state === 'no_internet') return '🟢 Без интернета';
        if (state === 'active_scheduled') return '🟢 Активность (расписание)';
        if (state === 'active_manual') return '🔵 Ручной режим';
        return '⚪ Нет данных';
    },

    // 🔥 Хелпер для правильного парсинга времени UTC в JS
    _parseUTC(dateStr) {
        // Если в строке нет 'Z', принудительно добавляем, чтобы JS понял, что это UTC
        if (!dateStr) return new Date();
        const isoStr = dateStr.includes('T') ? dateStr : dateStr.replace(' ', 'T');
        return new Date(isoStr.endsWith('Z') ? isoStr : isoStr + 'Z');
    },

    buildActivitySegments() {
        if (this.activityLog.length === 0) return [];

        // Начало выбранного дня в локальном времени
        const dayStart = new Date(this.activityDate + 'T00:00:00').getTime() / 1000;
        const dayEnd = dayStart + 24 * 3600;

        const segments = [];
        let currentSegment = null;

        // Сортируем по реальному времени UTC (используем наш парсер)
        const sorted = [...this.activityLog].sort(
            (a, b) => this._parseUTC(a.created_at) - this._parseUTC(b.created_at)
        );

        for (const entry of sorted) {
            const entryTime = this._parseUTC(entry.created_at).getTime() / 1000;
            const time = entryTime - dayStart;

            if (time < 0) continue;
            const clampedTime = Math.min(time, 24 * 3600);

            const state = entry.state || 'active_scheduled';

            if (!currentSegment) {
                currentSegment = { start: clampedTime, end: clampedTime, state: state };
            } else if (state === currentSegment.state) {
                currentSegment.end = clampedTime;
            } else {
                segments.push(currentSegment);

                if (clampedTime - currentSegment.end > 300) {
                    segments.push({ start: currentSegment.end, end: clampedTime, state: '' });
                }

                currentSegment = { start: clampedTime, end: clampedTime, state: state };
            }
        }

        if (currentSegment) {
            const nowSec = new Date().getTime() / 1000;
            const maxEnd = Math.min(dayEnd, nowSec);

            // Обрезаем хвост по последнему heartbeat (5 минут)
            if (currentSegment.end < maxEnd && currentSegment.end > dayEnd - 3600) {
                currentSegment.end = Math.min(currentSegment.end + 300, maxEnd);
            }
            segments.push(currentSegment);
        }

        return segments;
    },

    // Форматирует UTC-строку из БД в локальное время (HH:MM:SS)
    formatEventTime(utcStr) {
        if (!utcStr) return '';
        const d = this._parseUTC(utcStr);
        return d.toLocaleTimeString('ru-RU', { hour: '2-digit', minute: '2-digit', second: '2-digit' });
    },
});