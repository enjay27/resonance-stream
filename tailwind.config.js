/** @type {import('tailwindcss').Config} */
module.exports = {
    content: [
        "./src/**/*.rs",
        "./index.html",
    ],
    theme: {
        extend: {
            colors: {
                'bpsr-green': '#00ff88',
            },
        },
    },
    plugins: [
        require("daisyui"), // The DaisyUI plugin
    ],
    daisyui: {
        themes: ["dark", "luxury", "night"],
    },
}