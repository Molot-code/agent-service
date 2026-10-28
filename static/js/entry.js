// static/js/entry.js
// Точка входа для esbuild. Все state-модули — просто импорты.
// Порядок важен: api.js первым (его используют все), format.js — до state.
import './api.js';
import './format.js';

import './state-auth.js';
import './state-defaults.js';
import './state-status.js';
import './state-users.js';
import './state-extra.js';
import './state-schedule.js';
import './state-settings.js';
import './state-logs.js';
import './state-manual.js';

import './app.js';