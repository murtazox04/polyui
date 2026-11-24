'use strict'

const { existsSync, readFileSync } = require('node:fs')
const { join } = require('node:path')

function isMusl() {
  if (process.report && typeof process.report.getReport === 'function') {
    const { header } = process.report.getReport()
    return !header.glibcVersionRuntime
  }
  try {
    return readFileSync('/usr/bin/ldd', 'utf8').includes('musl')
  } catch {
    return true
  }
}

function platformTag() {
  const { platform, arch } = process
  if (platform === 'linux') {
    return `linux-${arch}-${isMusl() ? 'musl' : 'gnu'}`
  }
  if (platform === 'win32') {
    return `win32-${arch}-msvc`
  }
  return `${platform}-${arch}`
}

function loadNative() {
  const tag = platformTag()
  const local = join(__dirname, `polyui.${tag}.node`)
  if (existsSync(local)) {
    return require(local)
  }
  try {
    return require(`@polyui/node-${tag}`)
  } catch (error) {
    throw new Error(
      `PolyUI has no native build for ${tag}. Run "npm run build" in bindings/node or install @polyui/node-${tag}.`,
      { cause: error },
    )
  }
}

const native = loadNative()

class PolyUIError extends Error {
  constructor(message, details = {}) {
    super(message)
    this.name = new.target.name
    Object.assign(this, details)
  }
}

class ValidationError extends PolyUIError {
  constructor(message, details = {}) {
    super(message, { problems: [], operation: 'show', ...details })
    this.code = 'POLYUI_VALIDATION'
  }
}

class CatalogError extends PolyUIError {
  constructor(message, details = {}) {
    super(message, { path: '', ...details })
    this.code = 'POLYUI_CATALOG'
  }
}

class ChartError extends PolyUIError {
  constructor(message, details = {}) {
    super(message, details)
    this.code = 'POLYUI_CHART'
  }
}

class ProtocolError extends PolyUIError {
  constructor(message, details = {}) {
    super(message, details)
    this.code = 'POLYUI_PROTOCOL'
  }
}

class RefRefusedError extends PolyUIError {
  constructor(message = 'refused', details = {}) {
    super(message, details)
    this.code = 'POLYUI_REF_REFUSED'
  }
}

const errorClasses = { ValidationError, CatalogError, ChartError, ProtocolError }

native.useErrorFactory((kind, message, details) => {
  const ErrorClass = errorClasses[kind] ?? PolyUIError
  return new ErrorClass(message, details)
})

module.exports = {
  Catalogs: native.Catalogs,
  Validator: native.Validator,
  Surface: native.Surface,
  Update: native.Update,
  Prepared: native.Prepared,
  Client: native.Client,
  toolDefinitions: native.toolDefinitions,
  openMessage: native.openMessage,
  patchMessage: native.patchMessage,
  closeMessage: native.closeMessage,
  closedSummary: native.closedSummary,
  compileChart: native.compileChart,
  chartSvg: native.chartSvg,
  describeChart: native.describeChart,
  PolyUIError,
  ValidationError,
  CatalogError,
  ChartError,
  ProtocolError,
  RefRefusedError,
}
