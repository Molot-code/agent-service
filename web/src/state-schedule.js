import { apiGet, apiPost } from './api.js';

export const ScheduleState = () => ({
    sameSchedule: true,
    _prevTodayLimitMin: null,
    _prevDailyLimitMin: null,
    _prevDayLimitMin: {},
    _prevTodayExtraMin: null,
    _prevTodayInternetExtraMin: null,

    checkSameSchedule() {
        if (!this.weekDays || this.weekDays.length < 7) return false;
        const first = this.weekDays[0];
        return this.weekDays.every(d =>
            d.start_time === first.start_time &&
            d.end_time === first.end_time &&
            d.limit_min === first.limit_min &&
            d.internet_limit_min === first.internet_limit_min
        );
    },

    syncDailyFromWeek() {
        if (!this.weekDays || this.weekDays.length === 0) return;
        const d = this.weekDays[0];
        this.dailySchedule = {
            start_time: d.start_time,
            end_time: d.end_time,
            limit_min: d.limit_min,
            internet_limit_min: d.internet_limit_min,
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

    onTodayScreenLimitChanged() {
        if (this._prevTodayLimitMin !== null &&
            this.today.internet_limit_min === this._prevTodayLimitMin &&
            this._prevTodayLimitMin !== this.today.limit_min) {
            this.today.internet_limit_min = this.today.limit_min;
        }
        this._prevTodayLimitMin = this.today.limit_min;
        if (this.today.internet_limit_min < 0) this.today.internet_limit_min = 0;
        this.saveToday();
    },

    onTodayInternetLimitChanged() {
        if (this.today.internet_limit_min < 0) this.today.internet_limit_min = 0;
        this.saveToday();
    },

    onTodayExtraChanged() {
        // Если поправки экрана и интернета были равны — интернет следует за экраном
        if (this._prevTodayExtraMin !== null &&
            this._prevTodayExtraMin === this._prevTodayInternetExtraMin &&
            this._prevTodayExtraMin !== this.today.extra_min) {
            this.today.internet_extra_min = this.today.extra_min;
        }
        this._prevTodayExtraMin = this.today.extra_min;
        this._prevTodayInternetExtraMin = this.today.internet_extra_min;
        this.saveToday();
    },

    onTodayInternetExtraChanged() {
        this._prevTodayExtraMin = this.today.extra_min;
        this._prevTodayInternetExtraMin = this.today.internet_extra_min;
        this.saveToday();
    },

    onDailyScreenLimitChanged() {
        if (this._prevDailyLimitMin !== null &&
            this.dailySchedule.internet_limit_min === this._prevDailyLimitMin &&
            this._prevDailyLimitMin !== this.dailySchedule.limit_min) {
            this.dailySchedule.internet_limit_min = this.dailySchedule.limit_min;
        }
        this._prevDailyLimitMin = this.dailySchedule.limit_min;
        if (this.dailySchedule.internet_limit_min < 0) this.dailySchedule.internet_limit_min = 0;
        this.applyDailyToAll();
        this.saveWeekSchedule();
    },

    onDailyInternetLimitChanged() {
        if (this.dailySchedule.internet_limit_min < 0) this.dailySchedule.internet_limit_min = 0;
        this.applyDailyToAll();
        this.saveWeekSchedule();
    },

    onDayScreenLimitChanged(idx) {
        const day = this.weekDays[idx];
        if (!day) return;
        const prev = this._prevDayLimitMin[idx];
        if (prev !== undefined &&
            day.internet_limit_min === prev &&
            prev !== day.limit_min) {
            day.internet_limit_min = day.limit_min;
        }
        this._prevDayLimitMin[idx] = day.limit_min;
        if (day.internet_limit_min < 0) day.internet_limit_min = 0;
        this.saveWeekSchedule();
    },

    onDayInternetLimitChanged(idx) {
        const day = this.weekDays[idx];
        if (!day) return;
        if (day.internet_limit_min < 0) day.internet_limit_min = 0;
        this.saveWeekSchedule();
    },

    async saveWeekSchedule() {
        if (this.savingSchedule) return;
        this.savingSchedule = true;
        try {
            if (this.sameSchedule) this.applyDailyToAll();

            const schedule = this.weekDays.map((day) => ({
                day_of_week: day.day_of_week,
                limit_sec: day.limit_min * 60,
                internet_limit_sec: day.internet_limit_min * 60,
                start_time: day.start_time,
                end_time: day.end_time,
            }));

            await apiPost(`/api/schedule/${this.selectedUser}`, { schedule });
            this.showStatus('✅ Расписание сохранено');
        } catch (e) {
            this.showStatus('❌ ' + e.message);
        } finally {
            this.savingSchedule = false;
        }
    },

    async saveToday() {
        if (this.savingToday) return;
        this.savingToday = true;
        try {
            await apiPost(`/api/today/${this.selectedUser}`, {
                limit_sec: this.today.limit_min * 60,
                internet_limit_sec: this.today.internet_limit_min * 60,
                extra_time_min: this.today.extra_min,
                extra_internet_time_min: this.today.internet_extra_min,
                start_time: this.today.start_time,
                end_time: this.today.end_time,
            });
            this.showStatus('✅ Сохранено');
        } catch (e) {
            this.showStatus('❌ ' + e.message);
        } finally {
            this.savingToday = false;
        }
    },
});