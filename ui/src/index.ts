/** Fiex iframe API. Browser runtime only; no Node.js or Tauri dependencies. */
import { FiexUiClient } from './client.js'

export * from './types.js'
export { FiexBridgeError } from './errors.js'
export { applyTheme } from './theme.js'
export { FiexUiClient } from './client.js'

export const fiex = new FiexUiClient()
