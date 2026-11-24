# @polyui/node

Node bindings for the PolyUI Rust core, built with napi-rs 3. A Node server (Express, Fastify, Hono, a queue worker) can use them to handle a model's `show_ui`, `update_ui` and `close_ui` calls the same way the Rust reference server does: check the call against the catalogs, resolve `$ref` values for the current user, compile charts, seal actions into handles, and build the `ui.open` / `ui.patch` messages and the summary the model gets back.

## Build

You need Rust and Node 18 or newer.

```sh
cd bindings/node
npm install
npm run build
npm test
```

`npm run build` runs `napi build --platform --release --strip` and leaves `polyui.<platform>.node` next to `index.js`. `index.js` is a small hand written loader: it picks the local build for your platform, or `@polyui/node-<platform>` when the package is installed from npm. `index.d.ts` holds the types.

The crate is a workspace member but not a default member, so `cargo build` and `cargo test` at the repository root skip it. `cargo clippy --workspace` builds it and needs no extra setup.

## A short tour

```js
const polyui = require('@polyui/node')

const validator = new polyui.Validator(polyui.Catalogs.builtin(), {
  actions: {
    pay_bill: {
      args: { bill: { kind: 'string', required: true, max: 32 } },
      requires_confirm: true,
    },
  },
  refSources: { account: { kind: 'string', max: 40 } },
  resolver: (user, source, key) => accountsFor(user).get(key) ?? null,
})

const tools = validator.toolDefinitions(['core@1', 'chart@1'])

try {
  const prepared = validator.prepareShow(toolInput, { user, declared: hello.catalogs })
  socket.send(JSON.stringify(prepared.messages[0]))
  remember(prepared.model, prepared.client, prepared.actions)
  return { type: 'tool_result', content: prepared.summary }
} catch (error) {
  if (error instanceof polyui.ValidationError) {
    return { type: 'tool_result', content: error.message, is_error: true }
  }
  throw error
}
```

`error.message` is the numbered problem list written for the model. `error.problems` has the same problems as objects (`at`, `message`, and where it applies `list`, `index`, `id`, `prop`, `path`, `field`), and `error.operation` is `show`, `update` or `close`.

| Name | What it does |
|---|---|
| `Catalogs` | `builtin()`, `withCatalog(objectOrJson)` for extension catalogs, `restrict(declared)`, `negotiate(declared)`, `tags`, `typeNames()`, `has(type)`. |
| `new Validator(catalogs, { actions, refSources, resolver, allowHttp })` | `showUi`, `updateUi`, `closeUi` take the tool input as an object or JSON text. |
| `validator.prepareShow` / `prepareUpdate` | The whole server pipeline in one call. Returns a `Prepared` with `model`, `client`, `messages`, `summary`, `actions`, `seq` and `touched`. |
| `validator.resolveRefs`, `compileCharts`, `sealActions`, `downgrade`, `summary`, `fallback` | The same steps one at a time. |
| `Surface`, `Update` | Validated results. `JSON.stringify(surface)` and `Surface.fromJSON()` let you keep a surface in a session store. |
| `toolDefinitions(catalogs, actions, strict)` | The Anthropic tool JSON for the three UI tools. |
| `openMessage`, `patchMessage`, `closeMessage`, `closedSummary` | Protocol messages and the close summary. |
| `compileChart`, `chartSvg`, `describeChart` | Chart scene, standalone SVG and the spoken summary. |
| `new Client(hello)` | The client core: `receive`, `setValue`, `fire`, `confirm`, `say`, `matchVoice`. |

All errors extend `PolyUIError`, which extends `Error`: `ValidationError`, `CatalogError` (bad catalog, action or ref source specs, with `path`), `ChartError` and `ProtocolError`. Each has a `code` such as `POLYUI_VALIDATION`.

Method names are camelCase. Data that mirrors the protocol keeps the wire names, so sealed actions look like `{ action_id, component, prop, action, args, with, requires_confirm }`, the same as `ui.action` messages.

### Refs and the resolver

Each ref source declares the kind of value it gives, in the catalog prop spec format. The resolver is called as `resolver(user, source, key)` with the `user` you passed to `prepareShow` or `resolveRefs`. Return the value, return `null` when there is nothing, or throw `polyui.RefRefusedError` when this user may not see it. The model only learns that the ref could not be resolved. Anything else the resolver throws is rethrown as is.

Only `prepared.client` and the messages carry resolved values. `prepared.model` keeps the `$ref` objects, and its summary shows them as `[ref]`.

### Actions

Action objects become handles like `{ action_id: 'act_3f2a...', confirm: true }`. Keep `prepared.actions` on the server and look the handle up when a `ui.action` comes in. Pass `nextId` to use your own ids.

## Not there yet

The resolver is called synchronously, so it cannot return a Promise. Load what it needs before calling `prepareShow`. Prebuilt per-platform npm packages are not published yet; build locally with `npm run build`.
