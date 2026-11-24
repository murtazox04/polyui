'use strict'

const { test } = require('node:test')
const assert = require('node:assert/strict')
const { makeValidator, polyui } = require('./helpers')

const GAS = {
  title: 'Gas use',
  kind: 'bar',
  unit: 'm3',
  rows: [
    { x: 'Jan', y: 31 },
    { x: 'Feb', y: 28 },
    { x: 'Mar', y: 19 },
  ],
}

test('compileChart returns a scene', () => {
  const scene = polyui.compileChart(GAS, { width: 480, height: 320 })
  assert.ok(scene.marks.length > 0)
  assert.equal(scene.view_box.width, 480)
  assert.equal(scene.view_box.height, 320)
})

test('chartSvg and describeChart', () => {
  const svg = polyui.chartSvg(GAS)
  assert.ok(svg.startsWith('<svg'))
  assert.ok(svg.trimEnd().endsWith('</svg>'))
  assert.notEqual(polyui.chartSvg(GAS, { dark: true }), svg)
  assert.ok(polyui.describeChart(GAS).startsWith('Gas use.'))
  assert.notEqual(polyui.describeChart(GAS, 'ru'), polyui.describeChart(GAS))
})

test('bad chart props throw ChartError', () => {
  assert.throws(() => polyui.compileChart({ title: 'x', kind: 'donut' }), polyui.ChartError)
  const rows = [...Array(1001).keys()].map((n) => ({ x: String(n), y: n }))
  assert.throws(() => polyui.compileChart({ ...GAS, rows }), (error) => error instanceof polyui.ChartError && /rows/.test(error.message))
})

test('charts are compiled into surfaces', () => {
  const validator = makeValidator()
  const payload = { root: 'chart', components: [{ id: 'chart', type: 'chart:Chart', props: GAS }] }
  const surface = validator.showUi(payload)
  const chart = validator.compileCharts(surface, { locale: 'en' }).get('chart')
  assert.ok(chart.props.scene.marks.length > 0)
  assert.ok(chart.say.startsWith('Gas use.'))
  assert.equal(surface.get('chart').props.scene, undefined)
  const prepared = validator.prepareShow(payload)
  assert.ok(prepared.messages[0].components[0].props.scene.marks.length > 0)
  assert.equal(prepared.model.get('chart').props.scene, undefined)
})
