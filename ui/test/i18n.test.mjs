import test from 'node:test'
import assert from 'node:assert/strict'
import { createI18n, normalizeLocale, translateMessage } from '../dist/i18n.js'
test('normalize supported language tags with deterministic fallback', () => {
  for (const [tag, locale] of [
    ['zh-Hant-HK', 'zh-TW'],
    ['zh_TW', 'zh-TW'],
    ['zh-CN', 'zh-CN'],
    ['en-US', 'en'],
    ['unknown', 'zh-CN'],
  ])
    assert.equal(normalizeLocale(tag), locale)
})
test('live language changes notify subscribers and preserve interpolation data', () => {
  const runtime = createI18n({
    en: { '打开 {0}': 'Open {0}' },
    'zh-TW': { '打开 {0}': '開啟 {0}' },
  })
  let calls = 0
  const dispose = runtime.subscribe(() => calls++)
  runtime.setLocale('en')
  assert.equal(runtime.t('打开 {0}', '中文.txt'), 'Open 中文.txt')
  runtime.setLocale('en')
  assert.equal(calls, 1)
  runtime.setLocale('zh-TW')
  assert.equal(runtime.t('打开 {0}', '中文.txt'), '開啟 中文.txt')
  dispose()
  runtime.setLocale('zh-CN')
  assert.equal(calls, 2)
  assert.equal(runtime.t('unknown'), 'unknown')
  assert.equal(runtime.t('constructor'), 'constructor')
})
test('formatted native errors preserve filenames and support reordered placeholders', () => {
  assert.equal(
    translateMessage('无法读取：中文.txt', { '无法读取：{error}': 'Cannot read: {error}' }),
    'Cannot read: 中文.txt',
  )
  assert.equal(
    translateMessage('中文.txt: 读取失败', { '{}: 读取失败': 'Failed to read {}' }),
    'Failed to read 中文.txt',
  )
  assert.equal(translateMessage('从 A 到 B', { '从 {0} 到 {1}': 'To {1} from {0}' }), 'To B from A')
  assert.equal(translateMessage('文档内容', {}), '文档内容')
})

import { attachFiexPanel } from '../dist/host.js'
test('host sends initial locale and live events on the existing channel', async () => {
  const previous = globalThis.window
  const host = new EventTarget()
  host.location = { origin: 'https://host' }
  globalThis.window = host
  let locale = 'en',
    changed,
    peer,
    initial,
    stop = false,
    ready
  const connected = new Promise((resolve) => {
    ready = resolve
  })
  const frame = {
    postMessage(message, _origin, ports) {
      if (message.type === 'fiex.bridge.probe') {
        const event = new Event('message')
        Object.assign(event, { source: frame, data: { type: 'fiex.bridge.listening' } })
        host.dispatchEvent(event)
      } else {
        initial = message.locale
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
  const dispose = attachFiexPanel({
    pluginId: 'test',
    sessionId: 'locale-session',
    origin: 'null',
    frame: { contentWindow: frame },
    allowedMethods: new Set(),
    dispatch: async () => {
      throw new Error('unreachable')
    },
    getLocale: () => locale,
    subscribeLocale: (fn) => {
      changed = fn
      return () => {
        stop = true
      }
    },
    onReady: ready,
  })
  try {
    await connected
    assert.equal(initial, 'en')
    const event = new Promise((resolve) => {
      peer.onmessage = (e) => resolve(e.data)
    })
    locale = 'zh-TW'
    changed()
    const message = await event
    assert.equal(message.name, 'locale.changed')
    assert.equal(message.payload, 'zh-TW')
    assert.equal(message.sessionId, 'locale-session')
    dispose()
    assert.equal(stop, true)
  } finally {
    dispose()
    peer?.close()
    globalThis.window = previous
  }
})
