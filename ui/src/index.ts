import { normalizeLocale, type Locale } from './i18n.js'
import { createClientApis } from './clientApi.js'
import { applyTheme, FiexBridgeError } from './protocol.js'
import type { BridgeEvent, BridgeResponse, JsonValue, Pending } from './protocol.js'

export type * from './resources.js'
export * from './i18n.js'
export { applyTheme, FiexBridgeError }
export type {
  BridgeError,
  BridgeRequest,
  BridgeResponse,
  JsonValue,
  ThemeSnapshot,
} from './protocol.js'

/** Fiex iframe API. Browser runtime only; no Node.js or Tauri dependencies. */
export class FiexUiClient {
  private port: MessagePort | undefined
  private sessionId = ''
  private currentLocale: Locale = 'zh-CN'
  private translateError = (value: string) => value
  private seq = 0
  private pending = new Map<string, Pending>()
  private listeners = new Map<string, Set<(value: JsonValue) => void>>()
  private readyPromise?: Promise<void>
  private closed = false
  private stopHandshake: (() => void) | undefined
  private readonly timeoutMs: number
  private readonly reportUnhandled: boolean
  private captured = 0
  private capturedAt = 0
  private readonly api: ReturnType<typeof createClientApis>

  constructor(options: number | { timeoutMs?: number; reportUnhandledErrors?: boolean } = {}) {
    this.timeoutMs = typeof options === 'number' ? options : (options.timeoutMs ?? 15000)
    this.reportUnhandled =
      typeof options === 'number' ? true : (options.reportUnhandledErrors ?? true)
    this.api = createClientApis({
      invoke: <T = JsonValue>(
        method: string,
        params?: unknown,
        options?: { signal?: AbortSignal; timeoutMs?: number },
      ) => this.invoke<T>(method, params, options),
      on: (name, handler) => this.on(name, handler),
      ready: () => this.ready(),
      getLocale: () => this.currentLocale,
      setTranslateError: (translator) => {
        this.translateError = translator
      },
    })
  }

  /** Failures the host cannot observe: uncaught UI exceptions. Rate limited so a
   * broken render loop cannot flood the host's diagnostic log. */
  private installErrorCapture(): void {
    if (!this.reportUnhandled || typeof window === 'undefined') return
    const report = (message: string, detail?: string) => {
      const now = Date.now()
      if (this.closed || this.captured >= 5 || now - this.capturedAt < 1000) return
      this.captured += 1
      this.capturedAt = now
      void this.errors.report(message, detail).catch(() => {})
    }
    window.addEventListener('error', (event) => {
      report(event.message || 'Uncaught error', event.error?.stack)
    })
    window.addEventListener('unhandledrejection', (event) => {
      const reason = event.reason as { message?: unknown; stack?: unknown } | undefined
      report(
        String(reason?.message ?? event.reason ?? 'Unhandled rejection'),
        String(reason?.stack ?? ''),
      )
    })
  }

  /** The host sends fiex.bridge.init with a transferred MessagePort after frame load. */
  ready(): Promise<void> {
    if (this.closed) return Promise.reject(new FiexBridgeError('closed', 'Panel is closed'))
    if (this.readyPromise) return this.readyPromise
    this.readyPromise = new Promise<void>((resolve, reject) => {
      const timeout = setTimeout(() => {
        window.removeEventListener('message', init)
        reject(new FiexBridgeError('timeout', 'Fiex UI Bridge handshake timed out'))
      }, this.timeoutMs)
      const init = (event: MessageEvent) => {
        if (event.source === window.parent && event.data?.type === 'fiex.bridge.probe') {
          window.parent.postMessage({ type: 'fiex.bridge.listening' }, '*')
          return
        }
        const data = event.data as {
          type?: string
          bridgeVersion?: string
          sessionId?: string
          nonce?: string
          locale?: string
        } | null
        const port = event.ports[0]
        if (
          event.source !== window.parent ||
          !data ||
          data.type !== 'fiex.bridge.init' ||
          data.bridgeVersion !== '1.0' ||
          !data.sessionId ||
          !data.nonce ||
          event.ports.length !== 1 ||
          !port
        )
          return
        clearTimeout(timeout)
        window.removeEventListener('message', init)
        this.currentLocale = normalizeLocale(data.locale)
        document.documentElement.lang = this.currentLocale
        this.port = port
        this.sessionId = data.sessionId
        port.onmessage = (message) => this.onMessage(message.data)
        port.onmessageerror = () => this.dispose('invalid_message')
        port.start()
        port.postMessage({
          type: 'fiex.bridge.ready',
          sessionId: data.sessionId,
          nonce: data.nonce,
          bridgeVersion: '1.0',
        })
        this.installErrorCapture()
        resolve()
      }
      this.stopHandshake = () => {
        clearTimeout(timeout)
        window.removeEventListener('message', init)
        reject(new FiexBridgeError('closed', 'Panel is closed'))
      }
      window.addEventListener('message', init)
      window.parent.postMessage({ type: 'fiex.bridge.listening' }, '*')
    })
    return this.readyPromise
  }

  private onMessage(raw: unknown): void {
    if (!raw || typeof raw !== 'object') return
    const message = raw as Partial<BridgeResponse> | Partial<BridgeEvent>
    if (message.bridgeVersion !== '1.0' || message.sessionId !== this.sessionId) return
    if (message.type === 'response' && typeof message.requestId === 'string') {
      const pending = this.pending.get(message.requestId)
      if (!pending) return
      this.pending.delete(message.requestId)
      clearTimeout(pending.timer)
      if (message.error)
        pending.reject(
          new FiexBridgeError(message.error.code, this.translateError(message.error.message)),
        )
      else pending.resolve(message.result ?? null)
    } else if (message.type === 'event' && typeof message.name === 'string') {
      if (message.name === 'locale.changed') {
        this.currentLocale = normalizeLocale(message.payload)
        document.documentElement.lang = this.currentLocale
      }
      if (message.name === 'theme.changed') applyTheme(message.payload ?? null)
      if (message.name === 'panel.visibility')
        document.documentElement.dataset.panelVisible = String(message.payload === true)
      this.listeners.get(message.name)?.forEach((listener) => listener(message.payload ?? null))
    }
  }

  async invoke<T = JsonValue>(
    method: string,
    params?: unknown,
    options?: { signal?: AbortSignal; timeoutMs?: number },
  ): Promise<T> {
    await this.ready()
    if (!this.port) throw new FiexBridgeError('closed', 'Panel is closed')
    if (options?.signal?.aborted) throw new FiexBridgeError('cancelled', 'Request cancelled')
    const requestId = 'ui-' + ++this.seq
    return new Promise<T>((resolve, reject) => {
      const abort = () => {
        this.port?.postMessage({
          type: 'cancel',
          bridgeVersion: '1.0',
          sessionId: this.sessionId,
          requestId,
        })
        const pending = this.pending.get(requestId)
        if (pending) {
          clearTimeout(pending.timer)
          this.pending.delete(requestId)
          pending.reject(new FiexBridgeError('cancelled', 'Request cancelled'))
        }
      }
      const timer = setTimeout(() => {
        this.pending.delete(requestId)
        options?.signal?.removeEventListener('abort', abort)
        reject(new FiexBridgeError('timeout', 'Host request timed out'))
      }, options?.timeoutMs ?? this.timeoutMs)
      this.pending.set(requestId, {
        timer,
        resolve: (value) => {
          options?.signal?.removeEventListener('abort', abort)
          resolve(value as T)
        },
        reject: (error) => {
          options?.signal?.removeEventListener('abort', abort)
          reject(error)
        },
      })
      options?.signal?.addEventListener('abort', abort, { once: true })
      this.port!.postMessage({
        bridgeVersion: '1.0',
        sessionId: this.sessionId,
        requestId,
        type: 'request',
        method,
        params,
      })
    })
  }

  on(name: string, handler: (data: JsonValue) => void): () => void {
    let handlers = this.listeners.get(name)
    if (!handlers) {
      handlers = new Set()
      this.listeners.set(name, handlers)
    }
    handlers.add(handler)
    return () => {
      handlers!.delete(handler)
      if (!handlers!.size) this.listeners.delete(name)
    }
  }

  get context() {
    return this.api.context
  }
  get i18n() {
    return this.api.i18n
  }
  get plugin() {
    return this.api.plugin
  }
  get clipboard() {
    return this.api.clipboard
  }
  get navigation() {
    return this.api.navigation
  }
  get preview() {
    return this.api.preview
  }
  get vfs() {
    return this.api.vfs
  }
  get tasks() {
    return this.api.tasks
  }
  get dialog() {
    return this.api.dialog
  }
  get panel() {
    return this.api.panel
  }
  get logs() {
    return this.api.logs
  }
  get errors() {
    return this.api.errors
  }
  get theme() {
    return this.api.theme
  }
  get events() {
    return this.api.events
  }

  dispose(reason = 'closed'): void {
    this.closed = true
    this.stopHandshake?.()
    this.stopHandshake = undefined
    this.port?.close()
    this.port = undefined
    for (const pending of this.pending.values()) {
      clearTimeout(pending.timer)
      pending.reject(new FiexBridgeError(reason, 'Fiex panel disconnected'))
    }
    this.pending.clear()
    this.listeners.clear()
  }
}

export const fiex = new FiexUiClient()
