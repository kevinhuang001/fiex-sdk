import { normalizeLocale, type Locale } from './i18n.js'
export * from './i18n.js'
/** Fiex iframe API. Browser runtime only; no Node.js or Tauri dependencies. */
export type ResourceRef = { mountId: string; nodeId: string }
export type ResourceMetadata = {
  name: string
  kind: string
  extension?: string
  mime?: string
  size?: number
  modified?: number
}
export type ResourceContext = { ref: ResourceRef; metadata?: ResourceMetadata }
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
type BridgeEvent = {
  bridgeVersion: '1.0'
  sessionId: string
  subscriptionId: string
  type: 'event'
  name: string
  payload: JsonValue
}
export type ExplorerContext = {
  locale?: Locale
  directory?: ResourceContext
  targets: ResourceContext[]
  windowId?: string
  paneId?: string
  /** Contribution id that opened this panel; lets one panel serve several
   * toolbar or context-menu entries without duplicating its HTML. */
  panelId?: string
  panelTitle?: string
  /** How the panel was opened. */
  source?: 'toolbar' | 'contextMenu'
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
type Pending = {
  resolve(v: JsonValue): void
  reject(e: Error): void
  timer: ReturnType<typeof setTimeout>
}
export class FiexUiClient {
  private port?: MessagePort
  private sessionId = ''
  private currentLocale: Locale = 'zh-CN'
  private translateError = (value: string) => value
  private seq = 0
  private pending = new Map<string, Pending>()
  private listeners = new Map<string, Set<(value: JsonValue) => void>>()
  private readyPromise?: Promise<void>
  private closed = false
  private stopHandshake?: () => void
  private readonly timeoutMs: number
  private readonly reportUnhandled: boolean
  private captured = 0
  private capturedAt = 0
  constructor(options: number | { timeoutMs?: number; reportUnhandledErrors?: boolean } = {}) {
    this.timeoutMs = typeof options === 'number' ? options : (options.timeoutMs ?? 15000)
    this.reportUnhandled =
      typeof options === 'number' ? true : (options.reportUnhandledErrors ?? true)
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
        const d = event.data as {
          type?: string
          bridgeVersion?: string
          sessionId?: string
          nonce?: string
          locale?: string
        } | null
        if (
          event.source !== window.parent ||
          !d ||
          d.type !== 'fiex.bridge.init' ||
          d.bridgeVersion !== '1.0' ||
          !d.sessionId ||
          !d.nonce ||
          event.ports.length !== 1
        )
          return
        clearTimeout(timeout)
        window.removeEventListener('message', init)
        this.currentLocale = normalizeLocale(d.locale)
        document.documentElement.lang = this.currentLocale
        this.port = event.ports[0]
        this.sessionId = d.sessionId
        this.port.onmessage = (e) => this.onMessage(e.data)
        this.port.onmessageerror = () => this.dispose('invalid_message')
        this.port.start()
        this.port.postMessage({
          type: 'fiex.bridge.ready',
          sessionId: d.sessionId,
          nonce: d.nonce,
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
    const msg = raw as Partial<BridgeResponse> | Partial<BridgeEvent>
    if (msg.bridgeVersion !== '1.0' || msg.sessionId !== this.sessionId) return
    if (msg.type === 'response' && typeof msg.requestId === 'string') {
      const pending = this.pending.get(msg.requestId)
      if (!pending) return
      this.pending.delete(msg.requestId)
      clearTimeout(pending.timer)
      if (msg.error)
        pending.reject(new FiexBridgeError(msg.error.code, this.translateError(msg.error.message)))
      else pending.resolve(msg.result ?? null)
    } else if (msg.type === 'event' && typeof msg.name === 'string') {
      if (msg.name === 'locale.changed') {
        this.currentLocale = normalizeLocale(msg.payload)
        document.documentElement.lang = this.currentLocale
      }
      if (msg.name === 'theme.changed') applyTheme(msg.payload ?? null)
      if (msg.name === 'panel.visibility')
        document.documentElement.dataset.panelVisible = String(msg.payload === true)
      this.listeners.get(msg.name)?.forEach((listener) => listener(msg.payload ?? null))
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
        resolve: (v) => {
          options?.signal?.removeEventListener('abort', abort)
          resolve(v as T)
        },
        reject: (e) => {
          options?.signal?.removeEventListener('abort', abort)
          reject(e)
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
      } satisfies BridgeRequest)
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
    return {
      get: () => this.invoke<ExplorerContext>('context.get'),
      subscribe: (h: (ctx: ExplorerContext) => void) =>
        this.on('context.changed', (v) => h(v as unknown as ExplorerContext)),
    }
  }
  get i18n() {
    return {
      getLocale: async (): Promise<Locale> => {
        await this.ready()
        return this.currentLocale
      },
      subscribe: (listener: (locale: Locale) => void) =>
        this.on('locale.changed', (value) => listener(normalizeLocale(value))),
      bind: async (runtime: {
        setLocale(value: unknown): void
        translateMessage?: (value: string) => string
      }): Promise<() => void> => {
        await this.ready()
        runtime.setLocale(this.currentLocale)
        this.translateError = runtime.translateMessage ?? ((value) => value)
        return this.on('locale.changed', (value) => runtime.setLocale(value))
      },
    }
  }
  get plugin() {
    return {
      invoke: <T = JsonValue>(
        command: string,
        args?: JsonValue,
        options?: { signal?: AbortSignal; timeoutMs?: number },
      ) =>
        this.invoke<T>(
          'plugin.invoke',
          { method: command, params: args ?? null },
          { timeoutMs: 120000, ...options },
        ),
      /** Read a native task created by this plugin (or by its own native side). */
      task: <T = JsonValue>(taskId: string) => this.invoke<T | null>('plugin.task', { taskId }),
    }
  }
  get clipboard() {
    return {
      writeText: (text: string) => this.invoke<null>('clipboard.writeText', { text }),
    }
  }
  get navigation() {
    return {
      /** Navigate the host explorer to a Core-owned path or virtual URI. */
      open: (path: string) => this.invoke<null>('navigation.open', { path }),
    }
  }
  get preview() {
    return {
      /** Mark a preview document ready so the host can hide its loading state. */
      ready: () => this.invoke<null>('preview.ready'),
    }
  }
  get vfs() {
    return {
      stat: (ref: ResourceRef) => this.invoke('vfs.stat', ref),
      list: (ref: ResourceRef, cursor?: string) => this.invoke('vfs.list', { ...ref, cursor }),
      open: (ref: ResourceRef) => this.invoke('vfs.open', ref),
    }
  }
  get tasks() {
    return {
      list: () => this.invoke('tasks.list'),
      create: (
        title: string,
        options: {
          detail?: string
          progress?: number
          indeterminate?: boolean
        } = {},
      ) => this.invoke<{ taskId: string }>('tasks.create', { title, ...options }),
      spinner: (title: string, options: { detail?: string } = {}) =>
        this.invoke<{ taskId: string }>('tasks.spinner', { title, ...options }),
      update: (
        taskId: string,
        patch: {
          label?: string
          detail?: string
          progress?: number
          indeterminate?: boolean
          done?: number
          total?: number
          step?: string
        },
      ) => this.invoke('tasks.update', { taskId, ...patch }),
      finish: (taskId: string, options: { error?: string; detail?: string } = {}) =>
        this.invoke('tasks.finish', { taskId, ...options }),
      cancel: (taskId: string) => this.invoke('tasks.cancel', { taskId }),
    }
  }
  /** Native file chooser is rendered by Fiex, never by plugin HTML. */
  get dialog() {
    return {
      openFile: (
        options: {
          title?: string
          directory?: ResourceRef
          extensions?: string[]
        } = {},
      ) => this.invoke<ResourceContext | null>('dialog.openFile', options),
      openFiles: (
        options: {
          title?: string
          directory?: ResourceRef
          extensions?: string[]
        } = {},
      ) => this.invoke<ResourceContext[] | null>('dialog.openFiles', options),
      openDirectory: (options: { title?: string; directory?: ResourceRef } = {}) =>
        this.invoke<ResourceContext | null>('dialog.openDirectory', options),
      saveFile: (
        options: {
          title?: string
          directory?: ResourceRef
          suggestedName?: string
          extensions?: string[]
        } = {},
      ) => this.invoke<ResourceContext | null>('dialog.saveFile', options),
    }
  }
  get panel() {
    return {
      resize: (width: number, height: number) => this.invoke('panel.resize', { width, height }),
      close: () => this.invoke('panel.close'),
    }
  }
  /** Best-effort logging through the native SDK. Never opens an error dialog. */
  get logs() {
    return {
      write: (level: 'trace' | 'debug' | 'info' | 'warn' | 'error', message: string) =>
        this.plugin.invoke('diagnostics.log', { level, message }).catch(() => null),
    }
  }
  /** Explicit render/background failure, not a second report of a rejected RPC. */
  get errors() {
    return {
      report: (message: string, detail?: string) =>
        this.plugin
          .invoke('diagnostics.report', { message, ...(detail === undefined ? {} : { detail }) })
          .catch(() => null),
    }
  }
  get theme() {
    return {
      get: () => this.invoke('theme.get'),
      subscribe: (h: (v: JsonValue) => void) => this.on('theme.changed', h),
    }
  }
  get events() {
    return {
      on: (name: string, h: (v: JsonValue) => void) => this.on(name, h),
    }
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
