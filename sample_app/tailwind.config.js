/** @type {import('tailwindcss').Config} */
module.exports = {
  content: [
    /**
    Scan all rust files for utilized css class names
    **/
    './src/*.rs',
  ],
  theme: {
    screens: {
      sm: '480px',
      md: '768px',
      lg: '976px',
      xl: '1440px',
    },
    extend: {
      colors: {
        primary: '#AA634B',
        secondary: '#000000',
        accent: '#657786',
        shade: '#8E8E8E',
        success: '#000000',
        warning: '#000000',
        error: '#000000',
        highlight: '#B14E2D',
        hover: '#B14E2D',
        active: '#B14E2D',
        background: '#555F9E',
      },
      fontSize: {
        'h1': '2.25rem',  // Equivalent to 36px
        'h2': '1.875rem', // Equivalent to 30px
        'h3': '1.5rem',   // Equivalent to 24px
      },
      fontFamily: {
        sans: ['Helvetica', 'Arial', 'sans-serif'],
        serif: ['Georgia', 'serif'],
      },
      spacing: {
        '72': '18rem',
        '84': '21rem',
        '96': '24rem',
      },
    },
  },
  plugins: [],
}

