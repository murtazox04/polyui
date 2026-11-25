'use strict'

const { test } = require('node:test')
const assert = require('node:assert/strict')
const { ACTIONS, billsCard, counter, lookup, makeValidator, polyui } = require('./helpers')

const USER = { id: 'u1' }

test('ref values never reach the model', () => {
  const seen = []
  const validator = makeValidator((user, source, key) => {
    seen.push([user, source, key])
    return lookup(user, source, key)
  })
  const prepared = validator.prepareShow(billsCard(), { user: USER, nextId: counter() })
  assert.deepEqual(seen, [[USER, 'account', 'power']])
  assert.ok(!prepared.summary.includes('**** 4471'))
  assert.ok(prepared.summary.includes('"[ref]"'))
  assert.ok(!JSON.stringify(prepared.model).includes('**** 4471'))
  assert.deepEqual(prepared.model.get('power').props.label, { $ref: 'account:power' })
  assert.equal(prepared.client.get('power').props.label, '**** 4471')
  const [message] = prepared.messages
  assert.equal(message.t, 'ui.open')
  assert.ok(JSON.stringify(message).includes('**** 4471'))
  assert.ok(!JSON.stringify(message).includes('$ref'))
})

test('resolveRefs on its own', () => {
  const validator = makeValidator()
  const surface = validator.showUi(billsCard())
  const resolved = validator.resolveRefs(surface, USER)
  assert.equal(resolved.get('power').props.label, '**** 4471')
  assert.deepEqual(surface.get('power').props.label, { $ref: 'account:power' })
})

test('refused refs become validation problems without the reason', () => {
  const validator = makeValidator()
  for (const user of [undefined, { id: 'someone else' }]) {
    assert.throws(
      () => validator.prepareShow(billsCard(), { user }),
      (error) => {
        assert.ok(error instanceof polyui.ValidationError)
        assert.equal(error.problems[0].id, 'power')
        assert.ok(!error.message.includes('not your account'))
        return true
      },
    )
  }
})

test('unknown keys and unregistered sources are rejected', () => {
  const validator = makeValidator()
  const text = (reference) => ({
    root: 't',
    components: [{ id: 't', type: 'Text', props: { text: { $ref: reference } } }],
  })
  assert.throws(() => validator.prepareShow(text('account:nope'), { user: USER }), polyui.ValidationError)
  assert.throws(
    () => validator.showUi(text('card:main')),
    (error) => error.problems[0].message.includes('card'),
  )
})

test('errors thrown by the resolver propagate unchanged', () => {
  const validator = makeValidator(() => {
    throw new RangeError('database is down')
  })
  const payload = {
    root: 't',
    components: [{ id: 't', type: 'Text', props: { text: { $ref: 'account:gas' } } }],
  }
  assert.throws(() => validator.prepareShow(payload), { name: 'RangeError', message: 'database is down' })
})

test('actions are sealed into handles', () => {
  const prepared = makeValidator().prepareShow(billsCard(), { user: USER, nextId: counter() })
  assert.deepEqual(prepared.actions, [
    {
      action_id: 'act_1',
      component: 'gas',
      prop: 'on_press',
      action: 'pay_bill',
      args: { bill: 'b_17' },
      with: [],
      requires_confirm: true,
    },
    {
      action_id: 'act_2',
      component: 'power',
      prop: 'on_press',
      action: 'pay_bill',
      args: { bill: 'b_18' },
      with: [],
      requires_confirm: true,
    },
  ])
  const shown = Object.fromEntries(prepared.messages[0].components.map((component) => [component.id, component]))
  assert.deepEqual(shown.gas.props.on_press, { action_id: 'act_1', confirm: true })
  assert.ok(!JSON.stringify(prepared.messages[0]).includes('b_17'))
  assert.equal(prepared.model.get('gas').props.on_press.action, 'pay_bill')
})

test('sealActions uses random ids by default and checks custom ones', () => {
  const validator = makeValidator()
  const surface = validator.showUi(billsCard())
  const { surface: sealed, actions } = validator.sealActions(surface)
  assert.equal(actions.length, 2)
  assert.match(actions[0].action_id, /^act_[0-9a-f]{16}$/)
  assert.notEqual(actions[0].action_id, actions[1].action_id)
  assert.equal(sealed.get('gas').props.on_press.action_id, actions[0].action_id)
  assert.throws(() => validator.sealActions(surface, () => 'not valid!'), /nextId/)
})

test('prepareUpdate builds data and patch messages', () => {
  const validator = makeValidator()
  const shown = validator.prepareShow(billsCard(), { user: USER, nextId: counter() })
  const changed = validator.prepareUpdate(
    {
      surface: 's1',
      upsert: [
        {
          id: 'gas',
          type: 'Button',
          props: { label: 'Gas paid', on_press: { action: 'pick_offer', args: { offer: 'o1' } } },
        },
      ],
      data: [{ path: '/paid', value: true }],
    },
    shown.model,
    shown.client,
    shown.seq,
    { user: USER, nextId: () => 'act_9' },
  )
  assert.equal(changed.seq, 3)
  assert.deepEqual(
    changed.messages.map((message) => message.t),
    ['ui.data', 'ui.patch'],
  )
  assert.deepEqual(changed.messages[1].upsert[0].props.on_press, { action_id: 'act_9', confirm: false })
  assert.deepEqual(changed.touched, ['gas'])
  assert.equal(changed.actions[0].action, 'pick_offer')
  assert.ok(changed.summary.includes('Button "Gas paid"'))
  assert.throws(
    () => validator.prepareUpdate({ surface: 's9', remove: ['gas'] }, shown.model, shown.client, shown.seq),
    /is not open/,
  )
})

test('charts are downgraded for clients without the chart catalog', () => {
  const payload = {
    root: 'chart',
    components: [
      {
        id: 'chart',
        type: 'chart:Chart',
        props: { title: 'Gas', kind: 'bar', rows: [{ x: 'Jan', y: 3 }, { x: 'Feb', y: 5 }] },
      },
    ],
  }
  const prepared = makeValidator().prepareShow(payload, { declared: ['core@1'] })
  assert.equal(prepared.messages[0].components[0].type, 'Text')
  assert.equal(prepared.client.get('chart').type, 'chart:Chart')
})

test('tool definitions have the Anthropic shape', () => {
  const catalogs = polyui.Catalogs.builtin()
  const tools = polyui.toolDefinitions(catalogs, ACTIONS, false, 'full')
  assert.deepEqual(
    tools.map((tool) => tool.name),
    ['show_ui', 'update_ui', 'close_ui'],
  )
  for (const tool of tools) {
    assert.equal(typeof tool.description, 'string')
    assert.equal(tool.input_schema.type, 'object')
    assert.equal(tool.input_schema.additionalProperties, false)
    assert.equal(tool.strict, undefined)
  }
  assert.deepEqual(tools[0].input_schema.required, ['root', 'components'])
  const strict = polyui.toolDefinitions(catalogs, ACTIONS, true, 'full')
  assert.ok(strict.some((tool) => tool.strict === true))
  assert.ok(JSON.stringify(tools[0]).includes('chart:Chart'))
  assert.ok(JSON.stringify(tools[0]).includes('pay_bill'))
  const coreOnly = polyui.toolDefinitions(catalogs.restrict(['core@1']), ACTIONS, false, 'full')
  assert.ok(!JSON.stringify(coreOnly[0]).includes('chart:Chart'))
  assert.deepEqual(makeValidator().toolDefinitions(['core@1'], false, 'full'), coreOnly)
})

test('compact is the default tool profile and much smaller', () => {
  const catalogs = polyui.Catalogs.builtin()
  const compact = polyui.toolDefinitions(catalogs, ACTIONS)
  const full = polyui.toolDefinitions(catalogs, ACTIONS, false, 'full')
  assert.deepEqual(compact, polyui.toolDefinitions(catalogs, ACTIONS, false, 'compact'))
  assert.deepEqual(compact[0].input_schema.required, ['root', 'components'])
  assert.ok(compact[0].description.includes('chart:Chart'))
  assert.ok(JSON.stringify(compact).length * 4 < JSON.stringify(full).length)
  assert.throws(() => polyui.toolDefinitions(catalogs, ACTIONS, false, 'tiny'), /compact/)
  assert.throws(() => polyui.toolDefinitions(catalogs, ACTIONS, true), /full profile/)
  assert.throws(() => makeValidator().toolDefinitions(undefined, true, 'compact'), /full profile/)
})

test('refKeys lists the declared refs to fetch', () => {
  const validator = makeValidator()
  assert.deepEqual(validator.refKeys(billsCard()), ['account:power'])
  assert.deepEqual(validator.refKeys(JSON.stringify(billsCard())), ['account:power'])
  const components = [
    { id: 't', type: 'Text', props: { text: { $ref: 'secret:x' } } },
    { id: 'u', type: 'Text', props: { text: { $ref: 'account:gas' } } },
    { id: 'v', type: 'Text', props: { text: { $ref: 'account:gas' } } },
  ]
  assert.deepEqual(validator.refKeys({ root: 't', components }), ['account:gas'])
  assert.deepEqual(validator.refKeys({ surface: 's1', upsert: components }), ['account:gas'])
  assert.deepEqual(validator.refKeys('not json'), [])
})

test('refs fetched with await replace the resolver', async () => {
  const calls = []
  const validator = makeValidator((user, source, key) => {
    calls.push([source, key])
    return 'wrong'
  })
  const fetch = async (ref) => ({ 'account:power': '**** 4471' })[ref]
  const wanted = validator.refKeys(billsCard())
  const values = await Promise.all(wanted.map(fetch))
  const refs = Object.fromEntries(wanted.map((ref, index) => [ref, values[index]]))
  const prepared = validator.prepareShow(billsCard(), { user: USER, nextId: counter(), refs })
  assert.deepEqual(calls, [])
  assert.equal(prepared.client.get('power').props.label, '**** 4471')
  assert.deepEqual(prepared.model.get('power').props.label, { $ref: 'account:power' })
  const resolved = validator.resolveRefs(validator.showUi(billsCard()), USER, refs)
  assert.equal(resolved.get('power').props.label, '**** 4471')
  assert.throws(() => validator.prepareShow(billsCard(), { user: USER, refs: {} }), polyui.ValidationError)
  assert.throws(() => validator.prepareShow(billsCard(), { user: USER, refs: ['account:power'] }), /refs must be/)
})

test('message builders', () => {
  const surface = makeValidator().showUi(billsCard())
  const opened = polyui.openMessage('s1', surface, 1, 'Your bills')
  assert.equal(opened.v, 1)
  assert.equal(opened.t, 'ui.open')
  assert.equal(opened.fallback, 'Your bills')
  assert.deepEqual(polyui.patchMessage('s1', 2, { remove: ['gas'], root: 'card' }), {
    v: 1,
    t: 'ui.patch',
    surface: 's1',
    seq: 2,
    remove: ['gas'],
    root: 'card',
  })
  assert.deepEqual(polyui.closeMessage('s1', 3), { v: 1, t: 'ui.close', surface: 's1', seq: 3 })
})

test('resolvers that return a Promise are rejected', () => {
  const validator = makeValidator(async () => '**** 0083')
  const payload = {
    root: 't',
    components: [{ id: 't', type: 'Text', props: { text: { $ref: 'account:gas' } } }],
  }
  assert.throws(() => validator.prepareShow(payload), /Promise/)
})
