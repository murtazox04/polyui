# Conformance fixtures

Language-neutral cases that pin down what `polyui-core` does. Every binding and renderer can replay them. The Rust harness is `crates/polyui-core/tests/conformance.rs`.

## `validate/*.json`

One model tool call checked against the catalogs.

| Field | Meaning |
|---|---|
| `description` | What the case shows. |
| `tool` | `show_ui` (default), `update_ui` or `close_ui`. |
| `input` | The tool input as the model wrote it. |
| `input_text` | Raw text instead of `input`, for broken JSON and limit cases. |
| `current` | For `update_ui`: the surface before the call (`root`, `components`, `data`). |
| `actions` | Registered actions: name to `{ "args": {prop specs}, "requires_confirm": bool }`. |
| `refs` | Registered `$ref` sources: source to `{ key: value }`. |
| `catalogs` | Optional list like `["core@1"]` the set is restricted to. |
| `extension` | Optional extension catalog added to the built-in ones. |
| `allow_http` | Host allows `http` URLs. Default `false`. |
| `expect` | Either `{ "problems": [lines] }`, the model-facing problem lines, or `{ "ok": true, "summary": ..., "fallback": ... }` with the summary as surface `s1` and the fallback text in locale `en`. |

## `client/*.json`

A client session: a `hello`, then steps run in order against one client.

| Step | Runs | `expect` |
|---|---|---|
| `receive` | `client.receive(message)` | the `RenderBatch` |
| `receive_text` | `client.receive(raw text)` | the `RenderBatch` |
| `set_value` | `client.set_value(surface, path, value)` | the `RenderBatch` |
| `fire` | `client.fire(surface, component, prop, detail)` | the `ui.action` message or `null` |
| `confirm` | `client.confirm(surface, action_id, nonce)` | the `ui.action` message or `null` |
| `say` | `client.say(surface)` | the list of utterances |
| `match_voice` | `client.match_voice(surface, text)` | the voice match or `null` |

## Updating expectations

`POLYUI_BLESS=1 cargo test -p polyui-core --test conformance` rewrites every `expect` with what the code does now. Review the diff before committing it: a changed expectation is a behavior change.
