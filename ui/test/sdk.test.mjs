import test from 'node:test'
import assert from 'node:assert/strict'
import { FiexBridgeError, FiexUiClient, fiex } from '../dist/index.js'

test('UI SDK exports independent bridge client', () => {
  assert.ok(fiex instanceof FiexUiClient)
  const err = new FiexBridgeError('permission_denied', 'denied')
  assert.equal(err.code, 'permission_denied')
  assert.equal(err.name, 'FiexBridgeError')
  assert.equal(err.message, 'denied')
})

import { attachFiexPanel } from '../dist/host.js'

test('host transfers a port only after the client listens, supports events and rejects unauthorized calls', async () => {
  const previous = globalThis.window
  const hostWindow = new EventTarget()
  hostWindow.location = { origin: 'https://host' }
  globalThis.window = hostWindow
  let peer
  let ready
  const didReady = new Promise((resolve) => {
    ready = resolve
  })
  const frameWindow = {
    postMessage(message, _origin, ports) {
      if (message.type === 'fiex.bridge.probe') {
        const event = new Event('message')
        Object.assign(event, {
          source: frameWindow,
          data: { type: 'fiex.bridge.listening' },
        })
        hostWindow.dispatchEvent(event)
      } else {
        peer = ports[0]
        peer.postMessage({
          type: 'fiex.bridge.ready',
          bridgeVersion: '1.0',
          sessionId: message.sessionId,
          nonce: message.nonce,
        })
      }
    },
  }
  let emit
  let unsubscribed = false
  const dispose = attachFiexPanel({
    pluginId: 'test',
    sessionId: 'session',
    origin: 'null',
    frame: { contentWindow: frameWindow },
    allowedMethods: new Set(['echo']),
    dispatch: async (_method, p) => p,
    onReady: ready,
    subscribe: (send) => {
      emit = send
      return () => {
        unsubscribed = true
      }
    },
  })
  try {
    await didReady
    const response = new Promise((resolve) => {
      peer.onmessage = (e) => resolve(e.data)
    })
    peer.postMessage({
      type: 'request',
      bridgeVersion: '1.0',
      sessionId: 'session',
      requestId: '1',
      method: 'forbidden',
    })
    assert.equal((await response).error.code, 'permission_denied')
    const event = new Promise((resolve) => {
      peer.onmessage = (e) => resolve(e.data)
    })
    emit('theme.changed', { mode: 'dark' })
    assert.equal((await event).name, 'theme.changed')
  } finally {
    dispose()
    peer?.close()
    globalThis.window = previous
  }
  assert.equal(unsubscribed, true)
})

test('disposed client fails promptly without starting another handshake', async () => {
  const client = new FiexUiClient()
  client.dispose()
  await assert.rejects(client.ready(), (error) => error.code === 'closed')
})
test('typed wrappers cover clipboard, navigation, preview readiness and plugin tasks', async () => {
  const client = new FiexUiClient()
  const calls = []
  client.invoke = async (method, params) => {
    calls.push([method, params])
    return null
  }
  await client.clipboard.writeText('copied')
  await client.navigation.open('fiex://mount/demo/')
  await client.preview.ready()
  await client.plugin.task('task-1')
  assert.deepEqual(calls, [
    ['clipboard.writeText', { text: 'copied' }],
    ['navigation.open', { path: 'fiex://mount/demo/' }],
    ['preview.ready', undefined],
    ['plugin.task', { taskId: 'task-1' }],
  ])
})

test('UI diagnostics use the native SDK endpoints without caller-supplied identity', async () => {
  const client = new FiexUiClient()
  const calls = []
  client.invoke = async (method, params) => { calls.push([method, params]); return null }
  await client.logs.write('info', 'ready')
  await client.errors.report('render failed', 'safe details')
  assert.deepEqual(calls, [
    ['plugin.invoke', { method: 'diagnostics.log', params: { level: 'info', message: 'ready' } }],
    ['plugin.invoke', { method: 'diagnostics.report', params: { message: 'render failed', detail: 'safe details' } }],
  ])
  client.invoke = async () => { throw new Error('host unavailable') }
  await client.logs.write('error', 'lost host')
  await client.errors.report('lost host')
})
