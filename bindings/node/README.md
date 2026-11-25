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
| `Catalogs` | `builtin()`, `withEmbed()` to add the `embed` catalog, `withCatalog(objectOrJson)` for extension catalogs, `restrict(declared)`, `negotiate(declared)`, `tags`, `typeNames()`, `has(type)`. |
| `new Validator(catalogs, { actions, refSources, resolver, allowHttp, uiResource })` | `showUi`, `updateUi`, `closeUi` take the tool input as an object or JSON text. |
| `validator.prepareShow` / `prepareUpdate` | The whole server pipeline in one call. Returns a `Prepared` with `model`, `client`, `messages`, `summary`, `actions`, `seq` and `touched`. |
| `validator.resolveRefs`, `compileCharts`, `attachApps`, `sealActions`, `downgrade`, `summary`, `fallback` | The same steps one at a time. |
| `Surface`, `Update` | Validated results. `JSON.stringify(surface)` and `Surface.fromJSON()` let you keep a surface in a session store. |
| `toolDefinitions(catalogs, actions, strict, profile)` | The Anthropic tool JSON for the three UI tools. `profile` is `'compact'` (default) or `'full'`; `strict` needs `'full'`. |
| `new ShowStream()`, `validator.feedShow(stream, chunk, options)`, `validator.finishShow(stream, payload, options)` | Stream a `show_ui` call: `ui.open` and `ui.patch` for each component as it arrives, then the full check. See [Streaming](https://github.com/murtazox04/polyui/blob/main/docs/bindings.md#streaming-a-show_ui-call). |
| `validator.refKeys(payload)` | The declared `source:key` refs a tool call uses, to fetch before `prepareShow(payload, { refs })`. |
| `openMessage`, `patchMessage`, `closeMessage`, `closedSummary` | Protocol messages and the close summary. |
| `compileChart`, `chartSvg`, `describeChart` | Chart scene, standalone SVG and the spoken summary. |
| `new Client(hello)` | The client core: `receive`, `setValue`, `fire`, `confirm`, `say`, `matchVoice`. |

All errors extend `PolyUIError`, which extends `Error`: `ValidationError`, `CatalogError` (bad catalog, action or ref source specs, with `path`), `ChartError` and `ProtocolError`. Each has a `code` such as `POLYUI_VALIDATION`.

Method names are camelCase. Data that mirrors the protocol keeps the wire names, so sealed actions look like `{ action_id, component, prop, action, args, with, requires_confirm }`, the same as `ui.action` messages.

### Refs and the resolver

Each ref source declares the kind of value it gives, in the catalog prop spec format. The resolver is called as `resolver(user, source, key)` with the `user` you passed to `prepareShow` or `resolveRefs`. Return the value, return `null` when there is nothing, or throw `polyui.RefRefusedError` when this user may not see it. The model only learns that the ref could not be resolved. Anything else the resolver throws is rethrown as is.

For async lookups, call `validator.refKeys(toolInput)`, await the values, and pass them as `refs: { 'source:key': value }` to `prepareShow`, `prepareUpdate` or `resolveRefs`; the resolver is then not called and a missing key is refused. See [Async lookups](https://github.com/murtazox04/polyui/blob/main/docs/bindings.md#async-lookups).

Only `prepared.client` and the messages carry resolved values. `prepared.model` keeps the `$ref` objects, and its summary shows them as `[ref]`.

### MCP Apps

With `Catalogs.builtin().withEmbed()` the model can show an MCP App through `embed:Embed`. Pass `uiResource` to the validator; it is called as `uiResource(user, uri)` for each distinct `ui://` URI in the call and returns the `resources/read` entry, `{ resource: entry, tool_input, tool_result }`, or `null` when this user has no such app. `prepareShow` and `prepareUpdate` check the entry (exact `text/html;profile=mcp-app` mime type, matching `uri`, `text` or a base64 `blob`) and put it in the server-only `app` prop of the client copy. A missing or wrong entry, or one too large for a 256 KiB message, is a `ValidationError` for the model. `prepared.model` and the summary never contain the app. When `declared` leaves out `embed@1`, no resource is read. [docs/embed.md](../../docs/embed.md) has the rules.

### Actions

Action objects become handles like `{ action_id: 'act_3f2a...', confirm: true }`. Keep `prepared.actions` on the server and look the handle up when a `ui.action` comes in. Pass `nextId` to use your own ids.

## Not there yet

The resolver and `uiResource` are called synchronously, so they cannot return a Promise. For refs, use `refKeys` and `refs` (above); load MCP App resources before calling `prepareShow`. Prebuilt per-platform npm packages are not published yet; build locally with `npm run build`.
