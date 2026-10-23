/** Fiex iframe API. Browser runtime only; no Node.js or Tauri dependencies. */
export type ResourceRef = { mountId: string; nodeId: string; version?: string };
export type JsonValue =
  null | string | number | boolean | JsonValue[] | { [key: string]: JsonValue };
export type BridgeError = { code: string; message: string };
export type BridgeRequest = {
  bridgeVersion: "1.0";
  sessionId: string;
  requestId: string;
  type: "request";
  method: string;
  params?: unknown;
};
export type BridgeResponse = {
  bridgeVersion: "1.0";
  sessionId: string;
  requestId: string;
  type: "response";
  result?: JsonValue;
  error?: BridgeError;
};
type BridgeEvent = {
  bridgeVersion: "1.0";
  sessionId: string;
  subscriptionId: string;
  type: "event";
  name: string;
  payload: JsonValue;
};
export type ExplorerContext = {
  directoryPath: string;
  selectionPaths: string[];
  windowId?: string;
  paneId?: string;
};
export class FiexBridgeError extends Error {
  constructor(
    readonly code: string,
    message: string,
  ) {
    super(message);
    this.name = "FiexBridgeError";
  }
}
type Pending = {
  resolve(v: JsonValue): void;
  reject(e: Error): void;
  timer: ReturnType<typeof setTimeout>;
};
export class FiexUiClient {
  private port?: MessagePort;
  private sessionId = "";
  private seq = 0;
  private pending = new Map<string, Pending>();
  private listeners = new Map<string, Set<(value: JsonValue) => void>>();
  private readyPromise?: Promise<void>;
  private closed = false;
  private stopHandshake?: () => void;
  constructor(private readonly timeoutMs = 15000) {}

  /** The host sends fiex.bridge.init with a transferred MessagePort after frame load. */
  ready(): Promise<void> {
    if (this.closed)
      return Promise.reject(new FiexBridgeError("closed", "Panel is closed"));
    if (this.readyPromise) return this.readyPromise;
    this.readyPromise = new Promise<void>((resolve, reject) => {
      const timeout = setTimeout(() => {
        window.removeEventListener("message", init);
        reject(
          new FiexBridgeError("timeout", "Fiex UI Bridge handshake timed out"),
        );
      }, this.timeoutMs);
      const init = (event: MessageEvent) => {
        if (
          event.source === window.parent &&
          event.data?.type === "fiex.bridge.probe"
        ) {
          window.parent.postMessage({ type: "fiex.bridge.listening" }, "*");
          return;
        }
        const d = event.data as {
          type?: string;
          bridgeVersion?: string;
          sessionId?: string;
          nonce?: string;
        } | null;
        if (
          event.source !== window.parent ||
          !d ||
          d.type !== "fiex.bridge.init" ||
          d.bridgeVersion !== "1.0" ||
          !d.sessionId ||
          !d.nonce ||
          event.ports.length !== 1
        )
          return;
        clearTimeout(timeout);
        window.removeEventListener("message", init);
        this.port = event.ports[0];
        this.sessionId = d.sessionId;
        this.port.onmessage = (e) => this.onMessage(e.data);
        this.port.onmessageerror = () => this.dispose("invalid_message");
        this.port.start();
        this.port.postMessage({
          type: "fiex.bridge.ready",
          sessionId: d.sessionId,
          nonce: d.nonce,
          bridgeVersion: "1.0",
        });
        resolve();
      };
      this.stopHandshake = () => {
        clearTimeout(timeout);
        window.removeEventListener("message", init);
        reject(new FiexBridgeError("closed", "Panel is closed"));
      };
      window.addEventListener("message", init);
      window.parent.postMessage({ type: "fiex.bridge.listening" }, "*");
    });
    return this.readyPromise;
  }
  private onMessage(raw: unknown): void {
    if (!raw || typeof raw !== "object") return;
    const msg = raw as Partial<BridgeResponse> | Partial<BridgeEvent>;
    if (msg.bridgeVersion !== "1.0" || msg.sessionId !== this.sessionId) return;
    if (msg.type === "response" && typeof msg.requestId === "string") {
      const pending = this.pending.get(msg.requestId);
      if (!pending) return;
      this.pending.delete(msg.requestId);
      clearTimeout(pending.timer);
      if (msg.error)
        pending.reject(new FiexBridgeError(msg.error.code, msg.error.message));
      else pending.resolve(msg.result ?? null);
    } else if (msg.type === "event" && typeof msg.name === "string") {
      if (msg.name === "theme.changed") applyTheme(msg.payload ?? null);
      if (msg.name === "panel.visibility")
        document.documentElement.dataset.panelVisible = String(
          msg.payload === true,
        );
      this.listeners
        .get(msg.name)
        ?.forEach((listener) => listener(msg.payload ?? null));
    }
  }
  async invoke<T = JsonValue>(
    method: string,
    params?: unknown,
    options?: { signal?: AbortSignal; timeoutMs?: number },
  ): Promise<T> {
    await this.ready();
    if (!this.port) throw new FiexBridgeError("closed", "Panel is closed");
    if (options?.signal?.aborted)
      throw new FiexBridgeError("cancelled", "Request cancelled");
    const requestId = "ui-" + ++this.seq;
    return new Promise<T>((resolve, reject) => {
      const abort = () => {
        this.port?.postMessage({
          type: "cancel",
          bridgeVersion: "1.0",
          sessionId: this.sessionId,
          requestId,
        });
        const pending = this.pending.get(requestId);
        if (pending) {
          clearTimeout(pending.timer);
          this.pending.delete(requestId);
          pending.reject(new FiexBridgeError("cancelled", "Request cancelled"));
        }
      };
      const timer = setTimeout(() => {
        this.pending.delete(requestId);
        options?.signal?.removeEventListener("abort", abort);
        reject(new FiexBridgeError("timeout", "Host request timed out"));
      }, options?.timeoutMs ?? this.timeoutMs);
      this.pending.set(requestId, {
        timer,
        resolve: (v) => {
          options?.signal?.removeEventListener("abort", abort);
          resolve(v as T);
        },
        reject: (e) => {
          options?.signal?.removeEventListener("abort", abort);
          reject(e);
        },
      });
      options?.signal?.addEventListener("abort", abort, { once: true });
      this.port!.postMessage({
        bridgeVersion: "1.0",
        sessionId: this.sessionId,
        requestId,
        type: "request",
        method,
        params,
      } satisfies BridgeRequest);
    });
  }
  on(name: string, handler: (data: JsonValue) => void): () => void {
    let handlers = this.listeners.get(name);
    if (!handlers) {
      handlers = new Set();
      this.listeners.set(name, handlers);
    }
    handlers.add(handler);
    return () => {
      handlers!.delete(handler);
      if (!handlers!.size) this.listeners.delete(name);
    };
  }
  get context() {
    return {
      get: () => this.invoke<ExplorerContext>("context.get"),
      subscribe: (h: (ctx: ExplorerContext) => void) =>
        this.on("context.changed", (v) => h(v as unknown as ExplorerContext)),
    };
  }
  get plugin() {
    return {
      invoke: <T = JsonValue>(
        command: string,
        args?: JsonValue,
        options?: { signal?: AbortSignal; timeoutMs?: number },
      ) =>
        this.invoke<T>(
          "plugin.invoke",
          { method: command, params: args ?? null },
          { timeoutMs: 120000, ...options },
        ),
    };
  }
  get vfs() {
    return {
      stat: (ref: ResourceRef) => this.invoke("vfs.stat", ref),
      list: (ref: ResourceRef, cursor?: string) =>
        this.invoke("vfs.list", { ...ref, cursor }),
      open: (ref: ResourceRef) => this.invoke("vfs.open", ref),
    };
  }
  get tasks() {
    return {
      list: () => this.invoke("tasks.list"),
      create: (
        title: string,
        options: {
          detail?: string;
          progress?: number;
          indeterminate?: boolean;
        } = {},
      ) =>
        this.invoke<{ taskId: string }>("tasks.create", { title, ...options }),
      spinner: (title: string, options: { detail?: string } = {}) =>
        this.invoke<{ taskId: string }>("tasks.spinner", { title, ...options }),
      update: (
        taskId: string,
        patch: {
          label?: string;
          detail?: string;
          progress?: number;
          indeterminate?: boolean;
          done?: number;
          total?: number;
          step?: string;
        },
      ) => this.invoke("tasks.update", { taskId, ...patch }),
      finish: (
        taskId: string,
        options: { error?: string; detail?: string } = {},
      ) => this.invoke("tasks.finish", { taskId, ...options }),
      cancel: (taskId: string) => this.invoke("tasks.cancel", { taskId }),
    };
  }
  /** Native file chooser is rendered by Fiex, never by plugin HTML. */
  get dialog() {
    return {
      openFile: (
        options: {
          title?: string;
          directory?: string;
          extensions?: string[];
        } = {},
      ) => this.invoke<string | null>("dialog.openFile", options),
      openFiles: (
        options: {
          title?: string;
          directory?: string;
          extensions?: string[];
        } = {},
      ) => this.invoke<string[] | null>("dialog.openFiles", options),
      openDirectory: (options: { title?: string; directory?: string } = {}) =>
        this.invoke<string | null>("dialog.openDirectory", options),
      saveFile: (
        options: {
          title?: string;
          directory?: string;
          suggestedName?: string;
          extensions?: string[];
        } = {},
      ) => this.invoke<string | null>("dialog.saveFile", options),
    };
  }
  get panel() {
    return {
      resize: (width: number, height: number) =>
        this.invoke("panel.resize", { width, height }),
      close: () => this.invoke("panel.close"),
    };
  }
  get theme() {
    return {
      get: () => this.invoke("theme.get"),
      subscribe: (h: (v: JsonValue) => void) => this.on("theme.changed", h),
    };
  }
  get events() {
    return {
      on: (name: string, h: (v: JsonValue) => void) => this.on(name, h),
    };
  }
  dispose(reason = "closed"): void {
    this.closed = true;
    this.stopHandshake?.();
    this.stopHandshake = undefined;
    this.port?.close();
    this.port = undefined;
    for (const pending of this.pending.values()) {
      clearTimeout(pending.timer);
      pending.reject(new FiexBridgeError(reason, "Fiex panel disconnected"));
    }
    this.pending.clear();
    this.listeners.clear();
  }
}
export const fiex = new FiexUiClient();

export type ThemeSnapshot = {
  mode: "light" | "dark";
  tokens: Record<string, string>;
};
export function applyTheme(value: JsonValue): void {
  if (!value || typeof value !== "object" || Array.isArray(value)) return;
  const theme = value as unknown as ThemeSnapshot;
  if (theme.mode !== "dark" && theme.mode !== "light") return;
  document.documentElement.style.colorScheme = theme.mode;
  document.documentElement.classList.toggle("dark", theme.mode === "dark");
  for (const [key, token] of Object.entries(theme.tokens || {})) {
    if (
      /^[a-z][a-z0-9-]*$/.test(key) &&
      typeof token === "string" &&
      !/[;{}]|url\s*\(/i.test(token)
    )
      document.documentElement.style.setProperty("--" + key, token);
  }
}
