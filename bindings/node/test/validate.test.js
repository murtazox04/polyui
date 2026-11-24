'use strict'

const { test } = require('node:test')
const assert = require('node:assert/strict')
const { billsCard, makeValidator, polyui } = require('./helpers')

test('showUi returns a surface and a summary for the model', () => {
  const validator = makeValidator()
  const surface = validator.showUi(billsCard())
  assert.ok(surface instanceof polyui.Surface)
  assert.equal(surface.root, 'card')
  assert.deepEqual(
    surface.components.map((component) => component.id),
    ['card', 'list', 'gas', 'power'],
  )
  assert.equal(surface.get('gas').props.tone, 'primary')
  assert.equal(surface.get('nothing'), null)
  assert.equal(
    validator.summary(surface, 's1'),
    'surface s1 open: Card "Your bills" > List(2) > [Button "Gas 29 322 so\'m" (act), Button "[ref]" (act)]',
  )
})

test('showUi accepts JSON text', () => {
  const validator = makeValidator()
  const fromText = validator.showUi(JSON.stringify(billsCard()))
  assert.ok(fromText.equals(validator.showUi(billsCard())))
})

test('problems are thrown as ValidationError with the model text and a list', () => {
  const validator = makeValidator()
  const payload = {
    root: 'card',
    components: [
      { id: 'card', type: 'Card', children: ['missing'] },
      { id: 'pay', type: 'PayButton' },
    ],
  }
  assert.throws(
    () => validator.showUi(payload),
    (error) => {
      assert.ok(error instanceof polyui.ValidationError)
      assert.ok(error instanceof polyui.PolyUIError)
      assert.ok(error instanceof Error)
      assert.equal(error.name, 'ValidationError')
      assert.equal(error.code, 'POLYUI_VALIDATION')
      assert.equal(error.operation, 'show')
      assert.match(error.message, /^3 problems, nothing was shown:\n1\. /)
      assert.equal(error.problems.length, 3)
      const unknown = error.problems.find((problem) => problem.id === 'pay')
      assert.equal(unknown.list, 'components')
      assert.equal(unknown.index, 1)
      assert.equal(unknown.at, 'components[1] id "pay"')
      assert.match(unknown.message, /is not in the catalog/)
      for (const problem of error.problems) {
        assert.ok(error.message.includes(problem.at))
      }
      return true
    },
  )
})

test('invalid JSON text is a validation problem at the input', () => {
  assert.throws(
    () => makeValidator().showUi('{"root": '),
    (error) => error.problems[0].at === 'input' && /invalid JSON/.test(error.problems[0].message),
  )
})

test('action arguments are checked against the registry', () => {
  const payload = {
    root: 'pay',
    components: [
      {
        id: 'pay',
        type: 'Button',
        props: { label: 'Pay', on_press: { action: 'pay_bill', args: { bill: 17 } } },
      },
    ],
  }
  assert.throws(
    () => makeValidator().showUi(payload),
    (error) => {
      assert.deepEqual(error.problems, [
        {
          at: 'components[0] id "pay" prop "on_press.args.bill"',
          message: 'expected text, got a number.',
          list: 'components',
          index: 0,
          id: 'pay',
          prop: 'on_press.args.bill',
        },
      ])
      return true
    },
  )
})

test('updateUi, apply and closeUi', () => {
  const validator = makeValidator()
  const surface = validator.showUi({ ...billsCard(), surface: 's1' })
  const update = validator.updateUi(
    {
      surface: 's1',
      remove: ['power'],
      upsert: [{ id: 'list', type: 'List', children: ['gas'] }],
      data: [{ path: '/total', value: 29322 }],
    },
    surface,
  )
  assert.ok(update instanceof polyui.Update)
  assert.equal(update.surface, 's1')
  assert.deepEqual(update.remove, ['power'])
  assert.deepEqual(update.data, [{ op: 'set', path: '/total', value: 29322 }])
  const changed = surface.apply(update)
  assert.equal(changed.get('power'), null)
  assert.deepEqual(changed.data, { total: 29322 })
  assert.equal(validator.closeUi({ surface: 's1' }), 's1')
  assert.equal(polyui.closedSummary('s1'), 'surface s1 closed')
  assert.throws(() => validator.updateUi({ surface: 's1' }, surface), { operation: 'update' })
  assert.throws(() => validator.closeUi({}), { operation: 'close' })
})

test('surfaces survive a JSON round trip', () => {
  const validator = makeValidator()
  const surface = validator.showUi(billsCard())
  const stored = JSON.parse(JSON.stringify(surface))
  assert.ok(polyui.Surface.fromJSON(stored).equals(surface))
  assert.throws(() => polyui.Surface.fromJSON({ components: [] }), /root/)
})

test('bad registries throw CatalogError', () => {
  const catalogs = polyui.Catalogs.builtin()
  assert.throws(
    () => new polyui.Validator(catalogs, { actions: { pay: { requires_confirm: 'yes' } } }),
    (error) => error instanceof polyui.CatalogError && error.path === 'pay.requires_confirm',
  )
  assert.throws(
    () => new polyui.Validator(catalogs, { refSources: { account: { kind: 'nope' } } }),
    polyui.CatalogError,
  )
})

test('extension catalogs, restriction and downgrade', () => {
  const catalogs = polyui.Catalogs.builtin()
  const extension = {
    id: 'acme',
    version: 1,
    types: {
      Link: {
        description: 'A link.',
        role: 'button',
        props: {
          href: { kind: 'url', required: true },
          label: { kind: 'string', max: 40 },
        },
        alt: '{label}',
        say: '{label}',
      },
    },
  }
  const extended = catalogs.withCatalog(extension)
  assert.deepEqual(extended.tags, ['core@1', 'chart@1', 'acme@1'])
  assert.ok(extended.has('acme:Link'))
  assert.ok(!catalogs.has('acme:Link'))
  assert.equal(extended.withCatalog(JSON.stringify({ ...extension, id: 'beta' })).tags.at(-1), 'beta@1')
  assert.deepEqual(extended.restrict(['core@1', 'acme@1']).tags, ['core@1', 'acme@1'])
  assert.deepEqual(extended.negotiate(['acme@1', 'chart@2']), ['acme@1'])
  assert.throws(() => extended.withCatalog(extension), polyui.CatalogError)
  assert.throws(() => catalogs.withCatalog({ id: 'core', version: 1, types: {} }), polyui.CatalogError)
  const validator = new polyui.Validator(extended)
  const surface = validator.showUi({
    root: 'link',
    components: [
      { id: 'link', type: 'acme:Link', props: { href: 'https://example.com', label: 'Docs' } },
    ],
  })
  const downgraded = validator.downgrade(surface, ['core@1'])
  assert.equal(downgraded.get('link').type, 'Text')
  assert.equal(downgraded.get('link').props.text, 'Docs')
})
