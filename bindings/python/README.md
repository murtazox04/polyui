# polyui for Python

This package lets a Python server (FastAPI, Django, a plain worker) use the PolyUI Rust core. It does the same job the Rust reference server does around a model's `show_ui`, `update_ui` and `close_ui` calls: check them against the catalogs, resolve `$ref` values for the current user, compile charts, seal actions into handles, and build the `ui.open` / `ui.patch` messages plus the short summary the model gets back.

It is a native extension built with PyO3 and maturin. One abi3 wheel works on CPython 3.10 and newer.

## Build and install

You need Rust, [uv](https://docs.astral.sh/uv/) and maturin 1.16 or newer (`uv tool install maturin`).

```sh
cd bindings/python
uv venv .venv --python 3.12
uv pip install --python .venv/bin/python pytest ruff
VIRTUAL_ENV=$PWD/.venv maturin develop --uv --release
```

To get a wheel instead, run `maturin build --release --out dist`.

The binding crate is a workspace member but not a default member, so a plain `cargo build` or `cargo test` at the repository root skips it. `cargo clippy --workspace` does build it. If the first `python3` on your `PATH` is older than 3.9, point PyO3 at a newer one with `PYO3_PYTHON=$PWD/.venv/bin/python`.

## A short tour

```python
import polyui

validator = polyui.Validator(
    polyui.Catalogs.builtin(),
    actions={
        "pay_bill": {
            "args": {"bill": {"kind": "string", "required": True, "max": 32}},
            "requires_confirm": True,
        }
    },
    ref_sources={"account": {"kind": "string", "max": 40}},
    resolver=lambda user, source, key: accounts_for(user).get(key),
)

tools = validator.tool_definitions(["core@1", "chart@1"])

try:
    prepared = validator.prepare_show(tool_input, user=current_user, declared=hello["catalogs"])
except polyui.ValidationError as error:
    send_tool_result(str(error), is_error=True)
else:
    send_to_client(prepared.messages[0])
    remember(prepared.model, prepared.client, prepared.actions)
    send_tool_result(prepared.summary)
```

`str(error)` is the numbered problem list written for the model. `error.problems` has the same problems as dicts (`at`, `message`, and where it applies `list`, `index`, `id`, `prop`, `path`, `field`), and `error.operation` says which tool failed.

What you get:

| Name | What it does |
|---|---|
| `Catalogs` | `builtin()`, `with_embed()` to add the `embed` catalog, `with_catalog(json_or_dict)` for extension catalogs, `restrict(declared)`, `negotiate(declared)`, `tags`, `type_names()`. |
| `Validator(catalogs, actions, ref_sources, resolver, allow_http, ui_resource)` | `show_ui`, `update_ui`, `close_ui` take the tool input as a dict or JSON text. |
| `Validator.prepare_show` / `prepare_update` | The whole server pipeline in one call. Returns `Prepared` with `model`, `client`, `messages`, `summary`, `actions`, `seq` and `touched`. |
| `Validator.resolve_refs`, `compile_charts`, `attach_apps`, `seal_actions`, `downgrade`, `summary`, `fallback` | The same steps one at a time, if you want to run them yourself. |
| `Surface`, `Update` | Validated results. `to_dict()` and `Surface.from_dict()` let you keep them in a session store. |
| `tool_definitions(catalogs, actions, strict=False, profile="compact")` | The Anthropic tool JSON for `show_ui`, `update_ui` and `close_ui`. `profile="full"` puts every prop in the schema and is needed for `strict`. |
| `ShowStream`, `Validator.feed_show(stream, chunk, ...)`, `Validator.finish_show(stream, payload=None, ...)` | Stream a `show_ui` call: `ui.open` and `ui.patch` for each component as it arrives, then the full check. See [Streaming](https://github.com/murtazox04/polyui/blob/main/docs/bindings.md#streaming-a-show_ui-call). |
| `Validator.ref_keys(payload)` | The declared `source:key` refs a tool call uses, to fetch before `prepare_show(..., refs=...)`. |
| `open_message`, `patch_message`, `close_message`, `closed_summary` | Protocol messages and the close summary. |
| `compile_chart`, `chart_svg`, `describe_chart` | Chart scene, standalone SVG and the spoken summary. |
| `Client(hello)` | The client core: `receive`, `set_value`, `fire`, `confirm`, `say`, `match_voice`. Handy for tests and server side rendering checks. |

Errors all derive from `PolyUIError`: `ValidationError`, `CatalogError` (bad catalog, action or ref source specs, with `.path`), `ChartError`, `ProtocolError` (bad `hello`).

### Refs and the resolver

Every ref source declares the kind of value it gives, using the same prop spec format as the catalogs. The validator uses it to check that a `$ref` fits the prop before anything is resolved. The resolver is called as `resolver(user, source, key)` with whatever you passed as `user`. Return the value, return `None` when there is nothing, or raise `polyui.RefRefusedError` when this user may not see it. The model only ever learns that the ref could not be resolved. Any other exception from the resolver is raised again unchanged, so a database error does not turn into a validation problem.

For async lookups, call `validator.ref_keys(tool_input)`, await the values, and pass them as `refs={"source:key": value}` to `prepare_show`, `prepare_update` or `resolve_refs`; the resolver is then not called and a missing key is refused. See [Async lookups](https://github.com/murtazox04/polyui/blob/main/docs/bindings.md#async-lookups).

The model's copy of a surface (`prepared.model`) keeps the `$ref` objects, and its summary shows them as `[ref]`. Only `prepared.client` and the messages carry the resolved values.

### MCP Apps

With `Catalogs.builtin().with_embed()` the model can show an MCP App through `embed:Embed`. Pass `ui_resource=` to the validator; it is called as `ui_resource(user, uri)` for each distinct `ui://` URI in the call and returns the `resources/read` entry, a dict `{"resource": entry, "tool_input": ..., "tool_result": ...}`, or `None` when this user has no such app. `prepare_show` and `prepare_update` check the entry (exact `text/html;profile=mcp-app` mime type, matching `uri`, `text` or a base64 `blob`) and put it in the server-only `app` prop of the client copy. A missing or wrong entry, or one too large for a 256 KiB message, is a `ValidationError` for the model. `prepared.model` and the summary never contain the app. When `declared` leaves out `embed@1`, no resource is read. Exceptions from the callback are raised again unchanged. [docs/embed.md](../../docs/embed.md) has the rules.

### Actions

`seal_actions` and the `prepare_*` calls replace every action object with a handle like `{"action_id": "act_3f2a...", "confirm": true}`. The real action name and arguments come back in `prepared.actions`; keep them on the server and look them up when the client sends `ui.action`. Pass `next_id=` if you want your own ids.

## FastAPI example

`examples/fastapi_app.py` is a small app with a `POST /show_ui` endpoint that checks the payload and returns the `ui.open` message, or a 422 with the problems. FastAPI is an optional extra:

```sh
uv pip install --python .venv/bin/python fastapi uvicorn httpx
.venv/bin/uvicorn --app-dir examples fastapi_app:app --reload
```

## Tests

```sh
.venv/bin/python -m pytest
.venv/bin/ruff check . && .venv/bin/ruff format --check .
```

The client tests replay every fixture in `tests/conformance/client`, so the binding is checked against the same expectations as the Rust core. The example tests are skipped when FastAPI is not installed.

## Not there yet

`ui_resource` is synchronous; read MCP App resources before the call or run it in a worker thread. Async ref lookups go through `ref_keys` and `refs` (above).
