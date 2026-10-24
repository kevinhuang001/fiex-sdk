import type { JsonValue, ThemeSnapshot } from './types.js'

export function applyTheme(value: JsonValue): void {
  if (!value || typeof value !== 'object' || Array.isArray(value)) return
  const theme = value as unknown as ThemeSnapshot
  if (theme.mode !== 'dark' && theme.mode !== 'light') return
  document.documentElement.style.colorScheme = theme.mode
  document.documentElement.classList.toggle('dark', theme.mode === 'dark')
  for (const [key, token] of Object.entries(theme.tokens || {})) {
    if (
      /^[a-z][a-z0-9-]*$/.test(key) &&
      typeof token === 'string' &&
      !/[;{}]|url\s*\(/i.test(token)
    )
      document.documentElement.style.setProperty('--' + key, token)
  }
}
