// eslint.config.js
import htmlParser, { TEMPLATE_ENGINE_SYNTAX } from '@html-eslint/parser';
import alpinejs from 'eslint-plugin-alpinejs';

export default [
    // 1. Базовая конфигурация для всех JS файлов
    {
        files: ['**/*.js'],
        languageOptions: {
            ecmaVersion: 'latest',
            sourceType: 'module',
        },
        rules: {
            // Здесь можно добавить правила для чистого JS
        },
    },

    // 2. Конфигурация для HTML-шаблонов (Alpine.js + MiniJinja)
    {
        files: ['**/*.html'],
        languageOptions: {
            parser: htmlParser,
            parserOptions: {
                // Ключевое исправление: используем пресет NUNJUCKS для поддержки синтаксиса MiniJinja
                templateEngineSyntax: TEMPLATE_ENGINE_SYNTAX.NUNJUCKS,
            },
        },
        plugins: {
            alpinejs,
        },
        rules: {
            // Основное правило плагина Alpine: запрещает прямое обращение к DOM
            'alpinejs/no-raw-dom-access': 'error',
        },
    },
];