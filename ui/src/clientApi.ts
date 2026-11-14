import { normalizeLocale, type Locale } from './i18n.js'
import * as r from './resources.js'
import type { JsonValue } from './protocol.js'

type Invoke = <T = JsonValue>(
  method: string,
  params?: unknown,
  options?: { signal?: AbortSignal; timeoutMs?: number },
) => Promise<T>

type ClientApiAccess = {
  invoke: Invoke
  on: (name: string, handler: (data: JsonValue) => void) => () => void
  ready: () => Promise<void>
  getLocale: () => Locale
  setTranslateError: (translator: (value: string) => string) => void
}

export function createClientApis(access: ClientApiAccess) {
  const { invoke, on, ready } = access
  const apis = {
    context: {
      get: async () => r.parseExplorerContext(await invoke<unknown>('context.get')),
      subscribe: (handler: (context: r.ExplorerContext) => void) =>
        on('context.changed', (value) => handler(r.parseExplorerContext(value))),
    },
    i18n: {
      getLocale: async (): Promise<Locale> => {
        await ready()
        return access.getLocale()
      },
      subscribe: (listener: (locale: Locale) => void) =>
        on('locale.changed', (value) => listener(normalizeLocale(value))),
      bind: async (runtime: {
        setLocale(value: unknown): void
        translateMessage?: (value: string) => string
      }): Promise<() => void> => {
        await ready()
        runtime.setLocale(access.getLocale())
        access.setTranslateError(runtime.translateMessage ?? ((value) => value))
        return on('locale.changed', (value) => runtime.setLocale(value))
      },
    },
    plugin: {
      invoke: <T = JsonValue>(
        command: string,
        args?: JsonValue,
        options?: { signal?: AbortSignal; timeoutMs?: number },
      ) =>
        invoke<T>(
          'plugin.invoke',
          { method: command, params: args ?? null },
          { timeoutMs: 120000, ...options },
        ),
      /** Read a native task created by this plugin (or by its own native side). */
      task: <T = JsonValue>(taskId: string) => invoke<T | null>('plugin.task', { taskId }),
    },
    clipboard: {
      writeText: (text: string) => invoke<null>('clipboard.writeText', { text }),
    },
    navigation: {
      /** Navigate the host explorer to a Core-owned path or virtual URI. */
      open: (path: string) => invoke<null>('navigation.open', { path }),
    },
    preview: {
      /** Mark a preview document ready so the host can hide its loading state. */
      ready: () => invoke<null>('preview.ready'),
    },
    vfs: {
      stat: (ref: r.ResourceRef) => invoke('vfs.stat', r.parseResourceRef(ref)),
      list: (ref: r.ResourceRef, cursor?: string) =>
        invoke('vfs.list', { ...r.parseResourceRef(ref), cursor }),
      open: (ref: r.ResourceRef) => invoke('vfs.open', r.parseResourceRef(ref)),
    },
    tasks: {
      list: () => invoke('tasks.list'),
      create: (
        title: string,
        options: { detail?: string; progress?: number; indeterminate?: boolean } = {},
      ) => invoke<{ taskId: string }>('tasks.create', { title, ...options }),
      spinner: (title: string, options: { detail?: string } = {}) =>
        invoke<{ taskId: string }>('tasks.spinner', { title, ...options }),
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
      ) => invoke('tasks.update', { taskId, ...patch }),
      finish: (taskId: string, options: { error?: string; detail?: string } = {}) =>
        invoke('tasks.finish', { taskId, ...options }),
      cancel: (taskId: string) => invoke('tasks.cancel', { taskId }),
    },
    /** Native file chooser is rendered by Fiex, never by plugin HTML. */
    dialog: {
      openFile: (
        options: { title?: string; directory?: r.ResourceRef; extensions?: string[] } = {},
      ) =>
        invoke<unknown>('dialog.openFile', r.parseResourcePickerOptions(options)).then(
          r.parseResourceContextResult,
        ),
      openFiles: (
        options: { title?: string; directory?: r.ResourceRef; extensions?: string[] } = {},
      ) =>
        invoke<unknown>('dialog.openFiles', r.parseResourcePickerOptions(options)).then(
          r.parseResourceContextList,
        ),
      openDirectory: (options: { title?: string; directory?: r.ResourceRef } = {}) =>
        invoke<unknown>('dialog.openDirectory', r.parseResourcePickerOptions(options)).then(
          r.parseResourceContextResult,
        ),
      saveFile: (
        options: {
          title?: string
          directory?: r.ResourceRef
          suggestedName?: string
          extensions?: string[]
        } = {},
      ) =>
        invoke<unknown>('dialog.saveFile', r.parseResourcePickerOptions(options)).then(
          r.parseResourceContextResult,
        ),
    },
    panel: {
      resize: (width: number, height: number) => invoke('panel.resize', { width, height }),
      close: () => invoke('panel.close'),
    },
    /** Best-effort logging through the native SDK. Never opens an error dialog. */
    logs: {
      write: (level: 'trace' | 'debug' | 'info' | 'warn' | 'error', message: string) =>
        apis.plugin.invoke('diagnostics.log', { level, message }).catch(() => null),
    },
    /** Explicit render/background failure, not a second report of a rejected RPC. */
    errors: {
      report: (message: string, detail?: string) =>
        apis.plugin
          .invoke('diagnostics.report', { message, ...(detail === undefined ? {} : { detail }) })
          .catch(() => null),
    },
    theme: {
      get: () => invoke('theme.get'),
      subscribe: (handler: (value: JsonValue) => void) => on('theme.changed', handler),
    },
    events: {
      on: (name: string, handler: (value: JsonValue) => void) => on(name, handler),
    },
  }
  return apis
}
