/** @type {import('tailwindcss').Config} */
module.exports = {
  content: [
      './src/**/*.rs',
],
  theme: {
    extend: {},
  },
  plugins: [
    require('@tailwindcss/forms'),
    require('@tailwindcss/typography')
  ],
}
