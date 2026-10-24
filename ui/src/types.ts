export type ResourceRef = { mountId: string; nodeId: string; version?: string }
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
export type ExplorerContext = {
  directoryPath: string
  selectionPaths: string[]
  windowId?: string
  paneId?: string
  /** Contribution id that opened this panel; lets one panel serve several
   * toolbar or context-menu entries without duplicating its HTML. */
  panelId?: string
  panelTitle?: string
  /** How the panel was opened. */
  source?: 'toolbar' | 'contextMenu'
}
export type ThemeSnapshot = {
  mode: 'light' | 'dark'
  tokens: Record<string, string>
}
