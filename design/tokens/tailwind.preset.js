// CaptureMann Tailwind preset. Usage: presets: [require('./tokens/tailwind.preset.js')]
// Colors resolve to CSS variables from tokens.css so dark/light theming works via [data-theme].
module.exports = {
  darkMode: ['selector', '[data-theme="dark"]'],
  theme: {
    extend: {
      colors: {
        cm: {
          ground: 'var(--cm-ground)',
          sidebar: 'var(--cm-sidebar)',
          surface: 'var(--cm-surface)',
          'surface-2': 'var(--cm-surface-2)',
          raised: 'var(--cm-raised)',
          field: 'var(--cm-field)',
          border: 'var(--cm-border)',
          line: 'var(--cm-line)',
          text: 'var(--cm-text)',
          'text-2': 'var(--cm-text-2)',
          'text-3': 'var(--cm-text-3)',
          mist: 'var(--cm-mist)',
          faint: 'var(--cm-faint)',
          rec: 'var(--cm-rec)',
          'rec-hover': 'var(--cm-rec-hover)',
          'rec-soft': 'var(--cm-rec-soft)',
          'on-rec': 'var(--cm-on-rec)',
          agent: 'var(--cm-agent)',
          'agent-text': 'var(--cm-agent-text)',
          success: 'var(--cm-success)',
          warning: 'var(--cm-warning)',
          'primary-bg': 'var(--cm-primary-bg)',
          'primary-fg': 'var(--cm-primary-fg)',
          'primary-hover': 'var(--cm-primary-hover)',
        },
      },
      fontFamily: {
        sans: ['Geist', 'system-ui', '-apple-system', 'Segoe UI', 'sans-serif'],
        mono: ['Geist Mono', 'ui-monospace', 'SFMono-Regular', 'Consolas', 'monospace'],
      },
      borderRadius: { 'cm-xs': '6px', 'cm-sm': '8px', 'cm-md': '10px', 'cm-lg': '14px', 'cm-xl': '16px', 'cm-2xl': '20px', 'cm-dialog': '18px' },
      letterSpacing: { display: '-0.045em', heading: '-0.03em', label: '0.04em' },
      boxShadow: { float: '0 30px 80px rgba(0,0,0,0.6)', hud: '0 18px 50px rgba(0,0,0,0.6)' },
      width: { sidebar: '248px', rail: '64px', toast: '440px', inspector: '300px', launcher: '760px' },
    },
  },
};
