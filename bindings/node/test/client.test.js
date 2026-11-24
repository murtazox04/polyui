'use strict'

const { test } = require('node:test')
const assert = require('node:assert/strict')
const { billsCard, fixtureNames, loadFixture, makeValidator, polyui } = require('./helpers')

const HELLO = {
  v: 1,
  t: 'hello',
  catalogs: ['core@1', 'chart@1'],
  modalities: ['screen', 'voice'],
  locale: 'en',
}

function runStep(client, step) {
  if ('receive' in step) return client.receive(step.receive)
  if ('receive_text' in step) return client.receive(step.receive_text)
  if ('set_value' in step) {
    const { surface, path, value } = step.set_value
    return client.setValue(surface, path, value)
  }
  if ('fire' in step) {
    const { surface, component, prop, detail } = step.fire
    return client.fire(surface, component, prop, detail)
  }
  if ('confirm' in step) {
    const { surface, action_id: actionId, nonce } = step.confirm
    return client.confirm(surface, actionId, nonce)
  }
  if ('say' in step) return client.say(step.say)
  if ('match_voice' in step) {
    const { surface, text } = step.match_voice
    return client.matchVoice(surface, text)
  }
  throw new Error(`unknown step ${Object.keys(step).join(', ')}`)
}

test('open_basic fixture', () => {
  const fixture = loadFixture('client', 'open_basic.json')
  const client = new polyui.Client(fixture.hello)
  const [step] = fixture.steps
  assert.deepEqual(client.receive(step.receive), step.expect)
  assert.equal(client.receive(JSON.stringify(step.receive)).surface, 's1')
  assert.deepEqual(client.surfaces, ['s1'])
  assert.equal(client.fallback('s1'), step.expect.fallback)
  const action = client.fire('s1', 'pay', 'on_press')
  assert.equal(action.t, 'ui.action')
  assert.equal(action.action_id, 'act_4kq9')
})

for (const name of fixtureNames('client')) {
  test(`client conformance fixture ${name}`, () => {
    const fixture = loadFixture('client', name)
    const client = new polyui.Client(fixture.hello)
    fixture.steps.forEach((step, number) => {
      assert.deepEqual(runStep(client, step), step.expect, `${name} step ${number}`)
    })
  })
}

test('a bad hello throws ProtocolError', () => {
  assert.throws(() => new polyui.Client({ v: 1, t: 'hello' }), polyui.ProtocolError)
  assert.throws(() => new polyui.Client('{not json'), polyui.ProtocolError)
})

test('server output round trips through the client', () => {
  const prepared = makeValidator().prepareShow(billsCard(), { user: { id: 'u1' } })
  const client = new polyui.Client(HELLO)
  const batch = client.receive(prepared.messages[0])
  assert.deepEqual(
    batch.ops.filter((op) => op.op === 'create').map((op) => op.id),
    ['card', 'list', 'gas', 'power'],
  )
  const action = client.fire('s1', 'power', 'on_press')
  assert.equal(action.action_id, prepared.actions[1].action_id)
  assert.equal(client.confirm('s1', action.action_id, 'n_1'), null)
  const sheet = client.receive({
    v: 1,
    t: 'ui.confirm',
    surface: 's1',
    action_id: action.action_id,
    nonce: 'n_1',
    title: 'Pay power',
  })
  assert.equal(sheet.ops[0].op, 'confirm')
  assert.equal(client.confirm('s1', action.action_id, 'n_1').confirm, 'n_1')
  const utterances = client.say('s1')
  assert.equal(utterances[0].id, 'card')
  assert.ok(JSON.stringify(utterances).includes('**** 4471'))
})

test('inputs and voice', () => {
  const client = new polyui.Client(HELLO)
  client.receive({
    v: 1,
    t: 'ui.open',
    surface: 's1',
    seq: 1,
    root: 'form',
    components: [
      { id: 'form', type: 'Card', props: { title: 'Plan' }, children: ['phone', 'plan'] },
      { id: 'phone', type: 'Input', props: { label: 'Phone', value: '/phone' } },
      {
        id: 'plan',
        type: 'ChoiceGroup',
        props: {
          label: 'Plan',
          value: '/plan',
          options: [
            { id: 'basic', label: 'Basic' },
            { id: 'max', label: 'Max' },
          ],
        },
      },
    ],
    data: {},
  })
  const batch = client.setValue('s1', '/phone', '+998901234567')
  assert.equal(batch.ops[0].value, '+998901234567')
  assert.deepEqual(client.data('s1'), { phone: '+998901234567' })
  const found = client.matchVoice('s1', 'the second one')
  assert.equal(found.component, 'plan')
  assert.equal(found.option, 'max')
  assert.equal(client.data('s1').plan, 'max')
  assert.equal(client.matchVoice('s1', ''), null)
  assert.equal(client.fire('s1', 'nothing', 'on_press'), null)
})
