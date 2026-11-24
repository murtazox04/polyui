# PolyUI

PolyUI lets an LLM agent answer with real interface (cards, charts, forms and buttons) instead of only text, and checks every piece against a typed catalog before anything reaches the user. The same answer draws on a web page, in a native app, or reads aloud in a voice assistant with no screen.

[Documentation](https://murtazox04.github.io/polyui/) · [Live demo](https://murtazox04.github.io/polyui/demos/chat/) · [Protocol](spec/protocol.md)

<img src="docs/images/chat-answer.gif" width="640" alt="A chat answer arrives as streamed text, then a bills card with two stats, a bar chart and a pay button">

The demo above is the chat example replaying a canned turn in the browser with the real wasm core. The live demo link runs the same thing; no model is called.

## Why it exists

- **One answer for every modality.** Each component knows its plain text form (`alt`) and its spoken form (`say`). A screen client draws the surface, a voice client reads it in order, and a screen plus voice client does both and highlights what it is reading. "The second one" or "ikkinchisi" picks the second option without a trip to the model.
- **Friendly to small models.** Components are a flat list with ids, not a nested tree, so there are no brackets to lose track of. Tool schemas are closed objects. When a call is wrong the model gets a numbered list of problems with the component id and prop, and tries again. For local models there is a grammar that blocks most mistakes while the model writes (see [Local models](docs/decode.md); not yet run against a real local model).
- **Security first.** The model never writes markup, script, CSS or URLs that run. Private values such as account numbers are filled in on the server through `$ref` and never enter the model's context. Buttons carry sealed handles, so neither the model nor the client can change what an action does, and risky actions get a confirm sheet whose text the host writes. Each rule has named tests ([Security](docs/security.md)).
- **Dashboards from your own data.** The model writes read-only SQL over your CSV and Parquet files, and the numbers on screen come from the query, not from the model ([Dashboards](docs/dashboards.md)).
- **One Rust core everywhere.** Protocol types, catalog checks, bindings, `alt` and `say` text, URL rules and voice matching live in one crate. It runs as wasm in the browser and as a native module in Python and Node; a C interface for iOS is in progress. Renderers only draw.

## How it works

```mermaid
flowchart LR
    U[User types or speaks] --> S
    M[Model] -- show_ui, update_ui, close_ui --> S[PolyUI server]
    S -- numbered problems, retry --> M
    S -- resolve refs, compile charts, seal actions --> S
    S -- ui.open, ui.patch, ui.data --> C[Client core, Rust]
    C -- render ops --> R[Renderer: web, iOS, voice]
    R -- tap or voice match --> C
    C -- ui.action with a handle --> S
    S -- confirm text from the host --> R
    S -- run the action --> H[Your app]
```

The model calls three tools. The server checks each call with the Rust core; a call with problems goes back to the model and never reaches a client. A valid call has its `$ref` values resolved for the current user, its charts compiled to SVG scenes, and its actions replaced with handles. The client core applies the messages and hands the renderer a short list of create, update and remove operations. Details are in [Concepts](docs/concepts.md) and the [protocol](spec/protocol.md).

## Quick start

You need Rust (the toolchain in `rust-toolchain.toml` with the `wasm32-unknown-unknown` target), Node 22, the `wasm-bindgen` CLI 0.2.130 and `wasm-opt` from binaryen. The [quick start](docs/quickstart.md) has the install commands.

**With a model.** The bills demo is a household in Tashkent with gas, power, water and internet bills. It calls the Anthropic API, so it needs a key in an env file.

```sh
(cd examples/chat && npm install && npm run build)
cargo run --release -p polyui -- serve --env-file ~/.config/polyui/anthropic.env --static examples/chat/dist
```

Open http://127.0.0.1:8787/ and ask "What do we owe this month?"

**Without a key.** The chat example has a dev server that replays one canned conversation through the same page:

```sh
cd examples/chat && npm install && npm run dev
```

Open http://localhost:4190/, send any message, tap a bar, then pay.

**Check a call by hand.** `polyui validate` runs the same checks the server runs on a `show_ui` call:

```sh
cargo run -p polyui -- validate surface.json
```

```
2 problems, nothing was shown:
1. components[1] id "due" prop "value": required prop is missing.
2. components[2] id "pay": type "PayButton" is not in the catalog. Known: Badge, Button, Card, ChoiceGroup, ConfirmSheet, Divider, Image, Input, List, Row, Stat, Text, chart:Chart, chart:Dashboard.
```

<img src="docs/images/chat-pay.gif" width="640" alt="Tapping the July bar adds a comparison line, then Pay opens the confirm sheet and the card turns to Paid">

## Components

Three catalogs ship with PolyUI. `core` and `chart` are on by default; `embed` only when the host turns it on. Hosts can add their own catalogs under a namespace such as `acme:InvoiceCard`.

| Component | What it is | Main props |
|---|---|---|
| `Text` | A paragraph or heading | `text`, `style` (body, title, subtitle, caption, muted) |
| `Card` | A box that groups content under an optional title | `title`, `subtitle`, `tone` |
| `Row` | Children side by side, wrapping on narrow screens | `gap`, `align` |
| `List` | A vertical list, one child per item | `title`, `ordered` |
| `Stat` | One important number with a label | `label`, `value`, `unit`, `tone`, `hint` |
| `Badge` | A short status label | `text`, `tone` |
| `Image` | A picture from an https URL | `src`, `alt`, `aspect` |
| `Button` | Fires an action | `label`, `tone`, `on_press`, `disabled` |
| `ChoiceGroup` | Pick one or more options | `options`, `multiple`, `value`, `on_change` |
| `Input` | A text field that writes into the data model | `label`, `value`, `input`, `placeholder`, `max_length` |
| `ConfirmSheet` | A simple yes or no question | `title`, `body`, `on_confirm`, `on_cancel` |
| `Divider` | A thin line between sections | none |
| `chart:Chart` | Bar, line, area, pie or scatter chart; bars and points are tappable | `title`, `kind`, `rows`, `unit`, `on_select` |
| `chart:Dashboard` | A grid of charts and stats | `title`, `columns` |
| `embed:Embed` | An MCP App in a sandboxed frame | `resource` (`ui://`), `title`, `height`, `on_message` |

Every prop, limit and default is in [The catalogs](docs/catalogs.md). Charts are compiled on the server by `polyui-chart` into an SVG scene plus a hit map, so clients need no chart library.

## Numbers we have measured

| What | Result | Where it comes from |
|---|---|---|
| Wasm client core | 58 967 bytes brotli (168 024 raw), budget 61 440 | `bindings/wasm/build.mjs`, checked in CI |
| Web renderer `polyui.js` | 10 922 bytes brotli, budget 12 288; CSS 3 613 bytes | `js/polyui-web/scripts/size.mjs` |
| `show_ui` valid on the first try | 28 of 28 turns with tool calls, 24 of 24 with inline JSON, in English, Uzbek (Latin and Cyrillic) and Russian | [Benchmark](docs/benchmark.md), `claude-haiku-5-5`, 32 prompts, one run |
| Mean time to first `ui.open` | 5.1 s with tool calls, 6.4 s with inline JSON | same run |
| Cost of the 32 prompt run | $0.10 with tool calls, $0.06 inline | same run |
| First UI while the model is still writing | first `ui.open` at 4.9 s, `show_ui` input complete at 7.7 s; a call without streaming opened at 6.8 s | [Server guide](docs/server.md#streaming-ui-while-the-model-writes), one live run |
| Same, replayed with scripted timing | 0.32 s with streaming, 2.47 s without, for a 15 component card | `cargo test -p polyui-server --test streaming` |
| Grammar compile for all three UI tools | 4.82 ms, first token mask 1.04 ms | [Local models](docs/decode.md#how-fast-it-is), Apple M5 Pro |
| `polyui` release binary | 7.3 MB, 92.0 MB with the SQL `query` feature | [Dashboards](docs/dashboards.md#costs-and-limits) |

One benchmark run with one model is a small sample. The benchmark page lists what it can and cannot tell you.

## Documentation

The same pages are on the [documentation site](https://murtazox04.github.io/polyui/).

| Guide | Reference | Evidence |
|---|---|---|
| [Quick start](docs/quickstart.md) | [Protocol](spec/protocol.md) | [Security model and tests](docs/security.md) |
| [Concepts](docs/concepts.md) | [Catalog format](spec/catalog.md) | [Benchmark](docs/benchmark.md) |
| [Server guide](docs/server.md) | [The catalogs](docs/catalogs.md) | [Architecture decisions](docs/adr/0001-stack.md) |
| [Python and Node](docs/bindings.md) | [Renderer contract](spec/renderer.md) | |
| [Dashboards](docs/dashboards.md) | | |
| [Local models](docs/decode.md) | | |
| [Embedding MCP Apps](docs/embed.md) | | |
| [Platforms](docs/platforms.md) | | |
| [iOS](docs/ios.md) | | |

## Platforms

| Platform | Status |
|---|---|
| Rust server (`polyui-server`, the `polyui` command) | Done |
| Web: wasm core and the `polyui-web` renderer | Done |
| Python (PyO3) and Node (napi-rs) bindings | Done |
| iOS: C interface over the core and a SwiftUI renderer | In progress |
| Android (Compose) | Planned |
| Flutter | Planned |

Screenshots from the iOS example app, which is still in progress ([iOS](docs/ios.md)):

<p>
<img src="docs/images/ios-light.png" width="260" alt="The iOS example app in light mode, with the July bar selected">
<img src="docs/images/ios-dark.png" width="260" alt="The iOS example app in dark mode after paying">
</p>

## License

Apache-2.0. See [LICENSE](https://github.com/murtazox04/polyui/blob/main/LICENSE).
