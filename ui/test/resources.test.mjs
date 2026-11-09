import test from 'node:test'
import assert from 'node:assert/strict'
import {
  parseExplorerContext,
  parseResourceContextList,
  parseResourceContextResult,
  parseResourceRef,
} from '../dist/resources.js'

test('resource references are reconstructed from the generated Rust contract', () => {
  assert.deepEqual(parseResourceRef({ mountId: 'local', nodeId: 'opaque', path: '/secret' }), {
    mountId: 'local',
    nodeId: 'opaque',
  })
  assert.throws(() => parseResourceRef(null), TypeError)
  assert.throws(() => parseResourceRef({ mountId: 'local' }), TypeError)
})

test('resource context results validate refs and optional metadata', () => {
  assert.deepEqual(
    parseResourceContextResult({
      ref: { mountId: 'local', nodeId: 'opaque' },
      metadata: { name: 'notes.txt', kind: 'file', size: 12 },
      storagePath: '/secret',
    }),
    {
      ref: { mountId: 'local', nodeId: 'opaque' },
      metadata: {
        name: 'notes.txt',
        kind: 'file',
        size: 12,
      },
    },
  )
  assert.equal(parseResourceContextResult(null), null)
  assert.throws(
    () => parseResourceContextResult({ ref: { mountId: 'local', nodeId: 4 } }),
    TypeError,
  )
  assert.throws(
    () =>
      parseResourceContextResult({
        ref: { mountId: 'local', nodeId: 'opaque' },
        metadata: { name: 'notes.txt', kind: 'file', size: -1 },
      }),
    TypeError,
  )
  assert.throws(
    () =>
      parseResourceContextResult({
        ref: { mountId: 'local', nodeId: 'opaque' },
        metadata: { name: 'notes.txt', kind: 'file', size: 1.5 },
      }),
    TypeError,
  )
  assert.throws(() => parseResourceContextList({ targets: [] }), TypeError)
})

test('explorer contexts reject malformed nested resources and lifecycle data', () => {
  assert.deepEqual(
    parseExplorerContext({
      locale: 'en',
      targets: [{ ref: { mountId: 'local', nodeId: 'opaque' } }],
      source: 'toolbar',
    }),
    { locale: 'en', targets: [{ ref: { mountId: 'local', nodeId: 'opaque' } }], source: 'toolbar' },
  )
  assert.throws(() => parseExplorerContext({ targets: [{}] }), TypeError)
  assert.throws(() => parseExplorerContext({ targets: [], source: 'unknown' }), TypeError)
})
