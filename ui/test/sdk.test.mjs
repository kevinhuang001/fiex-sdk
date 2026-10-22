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
