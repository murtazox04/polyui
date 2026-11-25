'use strict'

const { test } = require('node:test')
const assert = require('node:assert/strict')
const { billsCard, counter, makeValidator, polyui } = require('./helpers')

const USER = { id: 'u1' }

function chunks(text, size = 15) {
  const out = []
  for (let at = 0; at < text.length; at += size) out.push(text.slice(at, at + size))
  return out
}

function streamAll(validator, payload, options) {
  const stream = new polyui.ShowStream()
  const parts = chunks(JSON.stringify(payload))
  const sent = []
  parts.forEach((part, at) => {
    for (const message of validator.feedShow(stream, part, options)) sent.push([at, message])
  })
  return { stream, sent, total: parts.length }
}

test('components reach the client while the call streams', () => {
  const validator = makeValidator()
  const nextId = counter('h')
  const { stream, sent, total } = streamAll(validator, billsCard(), { user: USER, nextId })
  const kinds = sent.map(([, message]) => message.t)
  assert.equal(kinds[0], 'ui.open')
  assert.deepEqual([...new Set(kinds.slice(1))], ['ui.patch'])
  assert.ok(sent[0][0] < total / 2)
  assert.deepEqual(
    sent.map(([, message]) => message.seq),
    sent.map((_, index) => index + 1),
  )
  const streamed = JSON.stringify(sent.map(([, message]) => message))
  assert.ok(streamed.includes('**** 4471'))
  assert.ok(!streamed.includes('$ref'))
  assert.equal(stream.opened, 's1')
  assert.equal(stream.stopped, false)

  const prepared = validator.finishShow(stream, null, { user: USER, nextId })
  assert.deepEqual(prepared.messages, [])
  assert.equal(prepared.seq, stream.seq)
  const handles = prepared.actions.map((action) => action.action_id)
  assert.equal(handles.length, 2)
  for (const handle of handles) assert.ok(streamed.includes(handle))
  assert.equal(stream.finished, true)
  assert.throws(() => validator.feedShow(stream, '{}'), /finished/)
})

test('a bad component stops the stream and the final check decides', () => {
  const validator = makeValidator()
  const broken = billsCard()
  broken.components[3].type = 'Marquee'
  const { stream, sent } = streamAll(validator, broken, { user: USER })
  assert.ok(sent.length > 0)
  assert.equal(stream.stopped, true)
  assert.throws(() => validator.finishShow(stream, null, { user: USER }), polyui.ValidationError)
  assert.deepEqual(stream.closeMessage(), { v: 1, t: 'ui.close', surface: 's1', seq: stream.seq + 1 })
})

test('finish without an early open is prepareShow', () => {
  const validator = makeValidator()
  const stream = new polyui.ShowStream()
  assert.deepEqual(validator.feedShow(stream, JSON.stringify(billsCard()).slice(0, 20), { user: USER }), [])
  const prepared = validator.finishShow(stream, billsCard(), { user: USER, surfaceId: 's7' })
  assert.deepEqual(
    prepared.messages.map((message) => message.t),
    ['ui.open'],
  )
  assert.equal(stream.closeMessage(), null)
})
