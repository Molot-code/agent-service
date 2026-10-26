/** @type {import('tailwindcss').Config} */
module.exports = {
    content: [
        // Jinja-шаблоны: base.html, index.html и все partials (рекурсивно)
        "./templates/**/*.html",
        "./templates/**/*.jinja",
        "./templates/**/*.jinja2",
        "./templates/**/*.j2",
        // На случай, если шаблоны окажутся рядом со статикой
        "./static/**/*.html",
        // JS-файлы (Alpine-классы, динамика)
        "./static/**/*.js",
    ],
    theme: {
        extend: {},
    },
    plugins: [],
}