import type { Config } from 'tailwindcss';

// TraceWeft "Graphite" design tokens — a dark, IDE-style studio palette.
// See .agents/contexts/conventions/UI_UX_STYLE_PROFILE.md for the design language.
export default {
  content: ['./index.html', './src/**/*.{js,ts,jsx,tsx}'],
  theme: {
    extend: {
      colors: {
        // Surfaces (back-to-front)
        window: '#0b0d11',
        titlebar: '#14161b',
        nav: '#0e1015',
        surface: '#0c0e12',
        panel: { DEFAULT: '#14171d', 2: '#13161b' },
        code: '#090b0e',
        // Borders & dividers
        line: {
          DEFAULT: '#232831',
          inner: '#20242c',
          row: '#181b21',
          node: '#262b34',
          input: '#2e333c',
        },
        // Text
        ink: {
          hi: '#e7ebf2',
          mid: '#98a1b0',
          dim: '#5d6677',
          faint: '#3a4150',
          faint2: '#2f343d',
        },
        // Accent & semantic
        iris: { DEFAULT: '#7c83ff', text: '#a5a9ff' },
        flow: '#56cfe1',
        ok: '#4ade80',
        error: '#fb7185',
        warn: { DEFAULT: '#fbbf24', text: '#fde68a' },
        jsonstr: '#a5e887',
      },
      fontFamily: {
        sans: ['"IBM Plex Sans"', 'system-ui', 'sans-serif'],
        mono: ['"JetBrains Mono"', 'ui-monospace', 'SFMono-Regular', 'monospace'],
      },
      borderRadius: {
        window: '12px',
        panel: '10px',
        pill: '8px',
        chip: '6px',
      },
      boxShadow: {
        window: '0 24px 60px rgba(0,0,0,.40)',
        iris: '0 4px 14px rgba(124,131,255,.30)',
        node: '0 0 28px rgba(124,131,255,.22)',
      },
    },
  },
  plugins: [],
} satisfies Config;
