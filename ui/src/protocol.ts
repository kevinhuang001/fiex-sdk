/** Fiex iframe API. Browser runtime only; no Node.js or Tauri dependencies. */
export type JsonValue =
  null | string | number | boolean | JsonValue[] | { [key: string]: JsonValue }
export type BridgeError = { code: string; message: string }
export type BridgeRequest = {
  bridgeVersion: '1.0'
  sessionId: string
  requestId: string
  type: 'request'
  method: string
  params?: unknown
}
export type BridgeResponse = {
  bridgeVersion: '1.0'
  sessionId: string
  requestId: string
  type: 'response'
  result?: JsonValue
  error?: BridgeError
}
export type BridgeEvent = {
  bridgeVersion: '1.0'
  sessionId: string
  subscriptionId: string
  type: 'event'
  name: string
  payload: JsonValue
}

export class FiexBridgeError extends Error {
  constructor(
    readonly code: string,
    message: string,
  ) {
    super(message)
    this.name = 'FiexBridgeError'
  }
}

export type Pending = {
  resolve(v: JsonValue): void
  reject(e: Error): void
  timer: ReturnType<typeof setTimeout>
}

export type ThemeSnapshot = {
  mode: 'light' | 'dark'
  tokens: Record<string, string>
}

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
