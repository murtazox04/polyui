'use strict'

const { test } = require('node:test')
const assert = require('node:assert/strict')
const { polyui } = require('./helpers')

const URI = 'ui://bills/tariff'
const MIME = 'text/html;profile=mcp-app'
const HTML = '<!doctype html><p>Tariff calculator</p>'
const USER = { id: 'u1' }

function entry({ html = HTML, uri = URI, mime = MIME } = {}) {
  return { uri, mimeType: mime, text: html }
}

function tariffCard(resource = URI) {
  return {
    root: 'card',
    components: [
      { id: 'card', type: 'Card', props: { title: 'Tariffs' }, children: ['calc'] },
      { id: 'calc', type: 'embed:Embed', props: { resource, title: 'Tariff calculator', height: 'large' } },
    ],
  }
}

function make(uiResource) {
  return new polyui.Validator(polyui.Catalogs.builtin().withEmbed(), { uiResource })
}

test('the app reaches the client but not the model', () => {
  const seen = []
  const validator = make((user, uri) => {
    seen.push([user, uri])
    return { resource: entry(), tool_input: { usage_gb: 236 } }
  })
  const prepared = validator.prepareShow(tariffCard(), { user: USER, declared: ['core@1', 'embed@1'] })
  assert.deepEqual(seen, [[USER, URI]])
  const { app } = prepared.client.get('calc').props
  assert.equal(app.resource.text, HTML)
  assert.deepEqual(app.tool_input, { usage_gb: 236 })
  assert.equal(prepared.model.get('calc').props.app, undefined)
  assert.ok(!prepared.summary.includes(HTML))
  assert.ok(!prepared.summary.includes(URI))
  assert.ok(JSON.stringify(prepared.messages[0]).includes(HTML))
})

test('base64 blobs are decoded', () => {
  const blob = Buffer.from(HTML).toString('base64')
  const prepared = make((user, uri) => ({ uri, mimeType: MIME, blob })).prepareShow(tariffCard())
  const { resource } = prepared.client.get('calc').props.app
  assert.equal(resource.text, HTML)
  assert.equal(resource.blob, undefined)
})

test('resources that are not apps are problems for the model', () => {
  for (const found of [null, entry({ mime: 'text/html' }), entry({ uri: 'ui://bills/other' }), { uri: URI, mimeType: MIME }]) {
    assert.throws(
      () => make(() => found).prepareShow(tariffCard()),
      (error) => {
        assert.ok(error instanceof polyui.ValidationError)
        assert.equal(error.problems[0].id, 'calc')
        assert.equal(error.problems[0].prop, 'resource')
        assert.ok(error.problems[0].message.includes('is not an MCP App this host offers'))
        return true
      },
    )
  }
})

test('apps too large for one message are refused', () => {
  const big = 'x'.repeat(210 * 1024)
  assert.throws(
    () => make(() => entry({ html: big })).prepareShow(tariffCard()),
    (error) => error instanceof polyui.ValidationError && error.problems[0].message.includes('too large'),
  )
})

test('clients without embed never trigger a read', () => {
  const prepared = make(() => {
    throw new Error('no read expected')
  }).prepareShow(tariffCard(), { declared: ['core@1'] })
  assert.equal(prepared.messages[0].components[1].type, 'Text')
})

test('updateUi attaches apps to upserts', () => {
  const validator = make(() => entry())
  const shown = validator.prepareShow({ root: 'card', components: [{ id: 'card', type: 'Card', props: { title: 'Tariffs' } }] })
  const update = { surface: shown.surfaceId, upsert: tariffCard().components }
  const prepared = validator.prepareUpdate(update, shown.model, shown.client, shown.seq)
  assert.equal(prepared.client.get('calc').props.app.resource.uri, URI)
  assert.equal(prepared.model.get('calc').props.app, undefined)
})

test('callback errors, promises and bad shapes are thrown', () => {
  assert.throws(
    () =>
      make(() => {
        throw new Error('mcp server is down')
      }).prepareShow(tariffCard()),
    /mcp server is down/,
  )
  assert.throws(() => make(async () => entry()).prepareShow(tariffCard()), /Promise/)
  assert.throws(() => make(() => ({ resource: entry(), tool_input: 3 })).prepareShow(tariffCard()), /tool_input/)
})

test('attachApps on its own', () => {
  const validator = make(() => entry())
  const surface = validator.showUi(tariffCard())
  const attached = validator.attachApps(surface, USER)
  assert.equal(attached.get('calc').props.app.resource.mimeType, MIME)
  assert.equal(surface.get('calc').props.app, undefined)
  assert.ok(polyui.Catalogs.builtin().withEmbed().has('embed:Embed'))
  assert.ok(!polyui.Catalogs.builtin().has('embed:Embed'))
})
