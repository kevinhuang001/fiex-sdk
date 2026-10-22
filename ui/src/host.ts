/** Host-side port bridge. Embed only in Fiex, never in untrusted plugin content. */
import type { BridgeRequest, BridgeResponse, JsonValue } from './index.js'
export type HostSession = {
  pluginId: string
  sessionId: string
  origin: string
  frame: HTMLIFrameElement
  allowedMethods: ReadonlySet<string>
  dispatch(method: string, params: unknown): Promise<JsonValue>
}
export function attachFiexPanel(session: HostSession): () => void {
  const target = session.frame.contentWindow
  if (!target) throw new Error('iframe not loaded')
  if (session.origin === window.location.origin) throw new Error('plugin cannot share host origin')
  const channel = new MessageChannel()
  const nonce = crypto.randomUUID()
  let ready = false
  let closed = false
  let inFlight = 0
  const maxConcurrent = 16
  const timer = setTimeout(() => dispose(), 10000)
  const dispose = () => {
    if (closed) return
    closed = true
    clearTimeout(timer)
    channel.port1.close()
    channel.port2.close()
  }
  channel.port1.onmessage = async (e) => {
    if (closed) return
    const msg = e.data as Record<string, unknown> | null
    if (!msg || msg.sessionId !== session.sessionId) return
    if (!ready) {
      if (msg.type === 'fiex.bridge.ready' && msg.nonce === nonce && msg.bridgeVersion === '1.0') {
        ready = true
        clearTimeout(timer)
      } else dispose()
      return
    }
    if (
      msg.type !== 'request' ||
      msg.bridgeVersion !== '1.0' ||
      typeof msg.requestId !== 'string' ||
      typeof msg.method !== 'string'
    )
      return
    const req = msg as unknown as BridgeRequest
    const respond = (r: Partial<BridgeResponse>) => {
      if (!closed)
        channel.port1.postMessage({
          bridgeVersion: '1.0',
          sessionId: session.sessionId,
          requestId: req.requestId,
          type: 'response',
          ...r,
        } satisfies BridgeResponse)
    }
    if (!session.allowedMethods.has(req.method)) {
      respond({ error: { code: 'permission_denied', message: 'Method not authorized' } })
    } else if (inFlight >= maxConcurrent) {
      respond({ error: { code: 'busy', message: 'Too many requests' } })
    } else {
      ++inFlight
      try {
        respond({ result: await session.dispatch(req.method, req.params) })
      } catch (err) {
        respond({
          error: { code: 'internal', message: err instanceof Error ? err.message : 'Host error' },
        })
      } finally {
        --inFlight
      }
    }
  }
  channel.port1.start()
  // srcDoc sandboxed without allow-same-origin has an opaque origin; in that
  // one case targetOrigin must be '*'. The transferred channel is bound to
  // the specific frame contentWindow and a nonce, not globally broadcast.
  target.postMessage(
    { type: 'fiex.bridge.init', bridgeVersion: '1.0', sessionId: session.sessionId, nonce },
    session.origin === 'null' ? '*' : session.origin,
    [channel.port2],
  )
  return dispose
}
