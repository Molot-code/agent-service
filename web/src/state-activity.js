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
    activitySegments: [], // 👈 ВОТ ЭТО НУЖНО ДОБАВИТЬ
    // 🆕 Зум графика активности: 1 = 100% (в ширину экрана), до 800%
    activityChartZoom: 1,
    // ===== Выбранная дата для графика (YYYY-MM-DD) =====
    activityDate: getLocalToday(),

    // ===== Месяц/год для навигации в date picker =====
    activityPickerMonth: new Date().getMonth(),
    activityPickerYear: new Date().getFullYear(),


    // 🆕 Pinch-зум графика (только мобильные)
    _chartPinch: null, // { startDist, startZoom }
    isMobileView() {
        return window.matchMedia('(max-width: 767px)').matches;
    },
    onChartTouchStart(e) {
        // Работаем только на мобильных и только при 2 касаниях
        if (!this.isMobileView() || e.touches.length !== 2) return;
        const [a, b] = e.touches;
        const dx = a.clientX - b.clientX;
        const dy = a.clientY - b.clientY;
        this._chartPinch = {
            startDist: Math.hypot(dx, dy) || 1,
            startZoom: this.activityChartZoom,
        };
    },
    onChartTouchMove(e) {
        if (!this._chartPinch || e.touches.length !== 2) return;
        // Блокируем браузерный pinch-zoom страницы
        e.preventDefault();
        const [a, b] = e.touches;
        const dx = a.clientX - b.clientX;
        const dy = a.clientY - b.clientY;
        const dist = Math.hypot(dx, dy) || 1;
        const ratio = dist / this._chartPinch.startDist;
        const next = this._chartPinch.startZoom * ratio;
        // Шаг 0.25, границы 1..8
        this.activityChartZoom = Math.min(8, Math.max(1, Math.round(next * 4) / 4));
    },
    onChartTouchEnd() {
        this._chartPinch = null;
    },

    // ===== Загрузка активности за выбранную дату =====
    async loadActivityLog() {
        console.log('[activity] loadActivityLog START', {
            t: performance.now().toFixed(0),
            selectedUser: this.selectedUser,
            date: this.activityDate,
        });
        if (!this.selectedUser) {
            console.warn('[activity] loadActivityLog: no selectedUser, abort');
            return;
        }
        this.activityLoading = true;
        try {
            const res = await apiGet(
                `/api/activity_log/${this.selectedUser}?date=${this.activityDate}`
            );
            console.log('[activity] loadActivityLog response', {
                t: performance.now().toFixed(0),
                entries: res?.entries?.length,
            });
            this.activityLog = res.entries || [];
            this.buildActivitySegments(); // 👈 ВОТ ТАК ПРАВИЛЬНО
            console.log('[activity] activityLog assigned', {
                t: performance.now().toFixed(0),
                length: this.activityLog.length,
            });
        } catch (e) {
            console.error('[activity] loadActivityLog ERROR', e);
        } finally {
            this.activityLoading = false;
            console.log('[activity] loadActivityLog END', { t: performance.now().toFixed(0) });
        }
    },

    // 🆕 Вызывается при инициализации, когда пользователь загружен
    initActivity() {
        console.log('[activity] initActivity called', {
            t: performance.now().toFixed(0),
            selectedUser: this.selectedUser,
            logLength: this.activityLog?.length,
        });
        if (this.selectedUser && this.activityLog.length === 0) {
            this.loadActivityLog();
        }
    },

    // ===== Выбор даты в picker =====
    selectActivityDate(date) {
        this.activityDate = date;
        this.loadActivityLog();
    },

    // 🆕 Обертка, чтобы Alpine видел эту функцию
    getLocalToday() {
        return getLocalToday(); // вызывает глобальную функцию сверху файла
    },

    // Вспомогательный метод для форматирования локальной даты
    formatLocalDate(d) {
        return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`;
    },

    activityPrevDay() {
        const d = new Date(this.activityDate + 'T00:00:00');
        d.setDate(d.getDate() - 1);
        this.selectActivityDate(this.formatLocalDate(d));
    },

    activityNextDay() {
        const d = new Date(this.activityDate + 'T00:00:00');
        d.setDate(d.getDate() + 1);
        const nextDateStr = this.formatLocalDate(d);

        // Не даем уйти в будущее
        if (nextDateStr > this.getLocalToday()) return;

        this.selectActivityDate(nextDateStr);
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

    // 🆕 Зум графика
    chartZoomIn() {
        this.activityChartZoom = Math.min(8, +(this.activityChartZoom + 0.5).toFixed(1));
    },
    chartZoomOut() {
        this.activityChartZoom = Math.max(1, +(this.activityChartZoom - 0.5).toFixed(1));
    },
    chartZoomReset() {
        this.activityChartZoom = 1;
    },
    chartWidthStyle() {
        return `width: ${this.activityChartZoom * 100}%`;
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
        if (state === 'session_locked') return 'bg-gray-400';
        if (state === 'no_internet') return 'bg-lime-300';
        if (state === 'active_scheduled') return 'bg-green-500';
        if (state === 'active_manual') return 'bg-yellow-500';
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
        if (state === 'active_manual') return '🟡 Ручной режим';
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
        const t0 = performance.now();
        const segments = [];

        // 1. Если логов нет — возвращаем один серый блок на весь день
        if (this.activityLog.length === 0) {
            this.activitySegments = [{ start: 0, end: 86400, state: '' }];
            return this.activitySegments;
        }

        const dayStart = new Date(this.activityDate + 'T00:00:00').getTime() / 1000;
        const nowSec = new Date().getTime() / 1000;
        const nowRelative = nowSec - dayStart;

        // Если день в будущем — один серый блок
        if (nowRelative < 0) {
            this.activitySegments = [{ start: 0, end: 86400, state: '' }];
            return this.activitySegments;
        }

        // 2. Сортируем лог по времени
        const sorted = [...this.activityLog].sort(
            (a, b) => this._parseUTC(a.created_at) - this._parseUTC(b.created_at)
        );

        // 3. Преобразуем логи в массив { time, state }
        let events = [];
        for (const entry of sorted) {
            const entryTime = this._parseUTC(entry.created_at).getTime() / 1000;
            const timeSec = entryTime - dayStart;
            if (timeSec >= 0 && timeSec <= 86400) {
                events.push({ time: timeSec, state: entry.state || 'active_scheduled' });
            }
        }

        if (events.length === 0) {
            this.activitySegments = [{ start: 0, end: 86400, state: '' }];
            return this.activitySegments;
        }

        // 4. Формируем сегменты на основе разрывов > 5 минут (300 сек)
        const GAP = 300;
        let segStart = 0;
        let segState = '';

        // 🆕 Хелпер для умного пуша (схлопывает одинаковые состояния)
        const pushSegment = (start, end, state) => {
            if (end <= start) return; // Игнорируем пустые

            const last = segments[segments.length - 1];
            // Если последний сегмент имеет тот же цвет и примыкает вплотную — растягиваем его
            if (last && last.state === state && last.end === start) {
                last.end = end;
            } else {
                segments.push({ start, end, state });
            }
        };

        for (let i = 0; i < events.length; i++) {
            const ev = events[i];

            if (ev.time - segStart > GAP) {
                // Если разрыв большой — закрываем предыдущий сегмент
                if (ev.time > segStart) {
                    pushSegment(segStart, ev.time, segState);
                }
                segStart = ev.time;
                segState = ev.state;
            } else {
                // Если разрыв маленький, а состояние до этого было пустым — задаем его
                if (segState === '') {
                    segState = ev.state;
                }
            }
        }

        // 5. Закрываем последний сегмент (до "сейчас")
        let endTime = Math.min(nowRelative, 86400);
        if (endTime > segStart) {
            pushSegment(segStart, endTime, segState);
        }

        // 6. Добавляем серый блок на всё оставшееся время до 24:00
        if (endTime < 86400) {
            pushSegment(endTime, 86400, '');
        }

        // 7. Чистим нулевые сегменты (на всякий случай)
        const result = segments.filter(s => s.end > s.start);

        this.activitySegments = result.map(seg => {
            return {
                ...seg, // копируем старые данные сегмента (id, start, end...)
                color: this.getSegmentColor(seg), // сразу добавляем готовый цвет
                title: this.getSegmentTitle(seg)  // сразу добавляем готовый текст
            };
        });

        console.log(
            `%c📦 Сегменты: ${result.length} (Логов: ${events.length}) за ${(performance.now() - t0).toFixed(2)} мс`,
            'color: #e91e63; font-weight: bold;'
        );

        return result;
    },

    // Форматирует UTC-строку из БД в локальное время (HH:MM:SS)
    formatEventTime(utcStr) {
        if (!utcStr) return '';
        const d = this._parseUTC(utcStr);
        return d.toLocaleTimeString('ru-RU', { hour: '2-digit', minute: '2-digit', second: '2-digit' });
    },

    // Вспомогательный метод: какой статус был в конкретную 5-минутку?
    _getStateForSlot(slotIndex, sortedLog, dayStart) {
        const slotStart = slotIndex * 300;
        let state = '';

        for (const entry of sortedLog) {
            const timeSec = this._parseUTC(entry.created_at).getTime() / 1000 - dayStart;

            if (timeSec <= slotStart) {
                state = entry.state || 'active_scheduled';
            } else {
                break; // События идут по порядку, дальше смысла нет
            }
        }
        return state;
    },
    // Превращает состояние в CSS-класс для фона
    getSegmentColor(seg) {
        if (!seg.state) return 'bg-gray-200'; // "Нет данных" (пустая строка)
        return this._getCssClass(seg.state);
    },

    // Формирует текст всплывающей подсказки (тултипа) при наведении
    getSegmentTitle(seg) {
        // Переводим секунды в формат ЧЧ:ММ
        const fmt = (sec) => {
            const s = Math.max(0, Math.min(86400, Math.round(sec)));
            const h = Math.floor(s / 3600);
            const m = Math.floor((s % 3600) / 60);
            return `${String(h).padStart(2, '0')}:${String(m).padStart(2, '0')}`;
        };

        // Считаем длительность в минутах
        const durMin = Math.round((seg.end - seg.start) / 60);

        // Получаем красивую подпись из _getStateLabel
        const label = seg.state ? this._getStateLabel(seg.state) : '⚪ Нет данных';

        return `Время: ${fmt(seg.start)} - ${fmt(seg.end)}\n${label}: ${durMin} мин`;
    }
});