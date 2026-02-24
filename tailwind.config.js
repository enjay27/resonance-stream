/** @type {import('tailwindcss').Config} */
module.exports = {
    // CRITICAL: Point explicitly to the src directory
    content: {
        relative: true,
        files: [
            "./src/**/*.rs",
            "./index.html",
        ],
    },
    theme: {
        extend: {
            colors: {
                'bpsr-green': '#00ff88', // Verified from your previous UI
            },
        },
    },
    plugins: [],
}