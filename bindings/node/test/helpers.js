'use strict'

const { readFileSync, readdirSync } = require('node:fs')
const { join } = require('node:path')
const polyui = require('..')

const FIXTURES = join(__dirname, '..', '..', '..', 'tests', 'conformance')

const ACTIONS = {
  pay_bill: {
    args: { bill: { kind: 'string', required: true, max: 32 } },
    requires_confirm: true,
  },
  pick_offer: { args: { offer: { kind: 'string', required: true } } },
}

const SECRETS = new Map([
  ['account:gas', '**** 0083'],
  ['account:power', '**** 4471'],
])

function lookup(user, source, key) {
  if (!user || user.id !== 'u1') {
    throw new polyui.RefRefusedError('not your account')
  }
  return SECRETS.get(`${source}:${key}`) ?? null
}

function makeValidator(resolver = lookup) {
  return new polyui.Validator(polyui.Catalogs.builtin(), {
    actions: ACTIONS,
    refSources: { account: { kind: 'string', max: 40 } },
    resolver,
  })
}

function billsCard() {
  return {
    root: 'card',
    components: [
      { id: 'card', type: 'Card', props: { title: 'Your bills' }, children: ['list'] },
      { id: 'list', type: 'List', children: ['gas', 'power'] },
      {
        id: 'gas',
        type: 'Button',
        props: {
          label: "Gas 29 322 so'm",
          tone: 'primary',
          on_press: { action: 'pay_bill', args: { bill: 'b_17' } },
        },
      },
      {
        id: 'power',
        type: 'Button',
        props: {
          label: { $ref: 'account:power' },
          on_press: { action: 'pay_bill', args: { bill: 'b_18' } },
        },
      },
    ],
  }
}

function loadFixture(kind, name) {
  return JSON.parse(readFileSync(join(FIXTURES, kind, name), 'utf8'))
}

function fixtureNames(kind) {
  return readdirSync(join(FIXTURES, kind))
    .filter((name) => name.endsWith('.json'))
    .sort()
}

function counter(prefix = 'act_') {
  let next = 0
  return () => {
    next += 1
    return `${prefix}${next}`
  }
}

module.exports = {
  ACTIONS,
  billsCard,
  counter,
  fixtureNames,
  loadFixture,
  lookup,
  makeValidator,
  polyui,
}
