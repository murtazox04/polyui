# PolyUI protocol, version 1

This document is the source of truth for the wire format. Code in this repository follows it. If the code and this file disagree, the code is wrong.

## 1. The idea in one paragraph

An agent answers with UI instead of only text. It does that by calling three tools: `show_ui`, `update_ui` and `close_ui`. The server checks every call against a catalog of known components. A call that does not pass never reaches a client; the error goes back to the model so it can try again. A valid call becomes protocol messages that any client can draw: a web page, a phone app, or a voice assistant with no screen at all. Every component knows how to describe itself in plain text (`alt`) and in speech (`say`), so the same answer works everywhere.

```mermaid
sequenceDiagram
    participant M as Model
    participant S as PolyUI server
    participant C as Client
    C->>S: hello (catalogs, modalities)
    M->>S: show_ui(components, data)
    S->>S: validate against catalog
    alt invalid
        S-->>M: tool_result error, retry
    else valid
        S->>C: ui.open
    end
    C->>S: ui.action (user tapped or spoke)
    S->>M: tool_result with action and UI summary
```

## 2. Words used in this document

| Word | Meaning |
|---|---|
| surface | One piece of UI the agent opened. A chat answer usually has one surface. Surfaces have ids like `s1`. |
| component | One node in a surface: a button, a card, a list. |
| catalog | A named set of component types with the rules for their props. `core` and `chart` ship with PolyUI. Hosts can add their own. |
| data model | A JSON document that belongs to a surface. Components read from it through bindings. |
| host | The application that embeds PolyUI: it owns the model, the users, and the actions. |
| renderer | The client code that draws components for one platform. |

## 3. Envelope

Every message is one JSON object. Two fields are always present:

- `v`: protocol version, an integer. This document describes `1`.
- `t`: message type, a string.

A receiver that sees a `v` it does not support must not crash. A client shows the `fallback` text if the message has one, and sends `ui.error` with reason `unsupported_version`.

Unknown fields are ignored. Unknown `t` values are ignored by clients and answered with `ui.error` reason `unknown_message` by servers.

## 4. Handshake

The client speaks first.

```json
{
  "v": 1,
  "t": "hello",
  "client": { "name": "polyui-web", "version": "0.1.0" },
  "catalogs": ["core@1", "chart@1"],
  "modalities": ["screen", "voice"],
  "features": ["sync", "stream"],
  "locale": "uz-Latn"
}
```

- `catalogs`: what the renderer can draw, as `id@major`. The server must only send components from these catalogs. Anything else is sent as its `alt` text inside a `Text` component.
- `modalities`: any non-empty subset of `screen` and `voice`.
- `features`: optional abilities. `sync` means the client handles `ui.sync`. `stream` means it accepts surfaces that grow while the model is still writing.
- `locale`: BCP 47 tag. Used for `alt` and `say` templates and for voice matching.

The server answers with `welcome`:

```json
{ "v": 1, "t": "welcome", "session": "sess_8f2c", "catalogs": ["core@1", "chart@1"] }
```

`catalogs` in `welcome` is the intersection the server will actually use.

## 5. Components

A surface is a flat list of components. Children are referenced by id, never nested. This keeps streaming simple and keeps small models from losing track of brackets.

```json
{
  "id": "pay",
  "type": "Button",
  "props": { "label": "Pay 29 322 so'm", "tone": "primary", "on_press": { "action": "pay_bill", "args": { "bill": "b_17" } } },
  "children": []
}
```

| Field | Required | Notes |
|---|---|---|
| `id` | yes | 1 to 64 chars, `[A-Za-z0-9_-]`, unique in the surface. |
| `type` | yes | A catalog type. Core types are bare (`Button`). Extension types are namespaced (`acme:InvoiceCard`). |
| `props` | no | Checked against the catalog. Unknown props are an error. |
| `children` | no | Ids of child components, in order. Only allowed when the catalog says the type has children. |
| `alt` | no | Text fallback. When missing, the catalog template builds it from props. |
| `say` | no | Spoken form. When missing, the catalog template builds it from props. |
| `voice` | no | `{ "ordinal": 2, "synonyms": ["gaz", "hududgaz"] }`. Helps match speech like "the second one". |

The model rarely needs to write `alt`, `say` or `voice`. The catalog fills them in.

### 5.1 Surface rules

- Exactly one `root`. It must exist in the component list.
- Every id in `children` must exist.
- No cycles. A component has at most one parent.
- Depth from root is at most 12.
- At most 256 components per surface.
- Components that no parent reaches are allowed only in `ui.patch` while a surface is streaming. A finished surface with orphans is rejected.

### 5.2 Limits

| Limit | Value |
|---|---|
| One message, serialized | 256 KiB |
| One string prop | 4 KiB, or the catalog's own `max` |
| Data model, serialized | 128 KiB |
| Array length in props or data | 1000 |
| JSON nesting depth | 32 |

A server rejects anything above these limits before parsing the rest.

The data model limit is half the message limit because the whole data model travels in one `ui.open` or `ui.snapshot` together with the components. After every `show_ui`, every `update_ui` and every action that stores form values, the server checks the data model as it would sit in that envelope: nesting is counted from the envelope (so the data model itself can be at most 31 levels deep), every array holds at most 1000 items, every string and key holds at most 4 KiB, and the serialized data model is at most 128 KiB. The server also checks the whole `ui.open`, `ui.patch`, `ui.data` and `ui.snapshot` it is about to send against the client limits and rejects the tool call when one would not fit, so a client never has to refuse a message and ask for a resync that would fail again.

## 6. Bindings and references

A prop value can be a literal or one of two objects.

**`{"$bind": "/path"}`** reads from the surface data model. The path is a JSON Pointer (RFC 6901). The client resolves it, so data updates never resend the UI. If the path does not exist the prop is treated as missing, and a required missing prop makes the component render its `alt`.

**`{"$ref": "source:key"}`** is resolved on the server before anything leaves it. The host registers sources (for example `account`, `bill`) with a resolver that checks the current user's rights. The resolved value goes to the client. The model only ever sees the `$ref` string, never the value. This is how sensitive values stay out of the model's context.

Rules for `$ref`:

- `source` must be a registered source. Anything else is a validation error returned to the model.
- Every source declares up front the kind of value it gives, using the prop kinds of the catalog format, for example `{"kind": "string", "max": 16}`. When the model calls `show_ui` or `update_ui`, a `$ref` is checked against the prop using the declared kind only: the prop must accept every value of that kind (a string prop must allow the declared length, an enum must contain every declared value, a number range must contain the declared range, a URL prop must allow every declared scheme). The resolved value is never used to answer this check.
- The resolver gets the session's user and the key, and returns a value or a refusal. A refusal, an unknown key, or a value that does not fit the declared kind or the prop all produce the same problem text, `$ref could not be resolved.`, with no detail, so the model cannot learn anything about a value from the error.
- The UI summary sent back to the model (section 10) shows `$ref` props as `[ref]`, never as values.

## 7. Actions

Interactive props have the `action` kind, for example `on_press` on `Button`.

```json
{ "action": "pay_bill", "args": { "bill": "b_17" }, "with": ["/form/phone"] }
```

- `action` names an action the host registered on the server. Each registered action has an argument schema and a flag `requires_confirm`.
- `args` are checked against that schema when the model calls `show_ui`.
- `with` lists data model paths whose current client values are sent along when the action fires. This is how forms submit.

The model's `args` never travel to the client. When the server accepts a surface, it replaces each action object with a handle:

```json
{ "action_id": "act_4kq9", "confirm": true }
```

The server keeps `act_4kq9 -> pay_bill(bill = b_17)` in the session. The client can only send the handle back. It cannot change the arguments, and neither can the model after the fact.

### 7.1 Firing an action

```json
{ "v": 1, "t": "ui.action", "surface": "s1", "action_id": "act_4kq9", "values": { "/form/phone": "+998901234567" }, "source": "tap" }
```

`source` is `tap`, `voice` or `keyboard`. It is recorded, not trusted for security.

### 7.2 Confirmation

If the action has `requires_confirm`, the first `ui.action` does not run it. The server answers with a challenge whose text the host builds from the real arguments, not from anything the model wrote:

```json
{ "v": 1, "t": "ui.confirm", "surface": "s1", "action_id": "act_4kq9", "nonce": "n_77ad", "title": "Pay Hududgaz", "body": "29 322,19 so'm from card 4471", "confirm_label": "Pay", "expires_ms": 60000 }
```

The renderer shows this in its own confirm sheet, drawn by the renderer and not by the surface. The user confirms, and the client sends the action again with the nonce:

```json
{ "v": 1, "t": "ui.action", "surface": "s1", "action_id": "act_4kq9", "confirm": "n_77ad", "source": "tap" }
```

A nonce works once and expires. The model has no tool that fires actions, so it cannot skip this step.

### 7.3 Result

```json
{ "v": 1, "t": "ui.action.result", "surface": "s1", "action_id": "act_4kq9", "status": "ok" }
```

`status` is one of `ok`, `rejected`, `expired`, `error`. A `message` string is optional.

## 8. Server to client messages

All of these carry `surface` and a per-surface `seq` that starts at 1 and goes up by one. A client that sees a gap asks for a snapshot with `ui.resync`.

### `ui.open`

```json
{ "v": 1, "t": "ui.open", "surface": "s1", "seq": 1, "root": "card", "components": [], "data": {}, "fallback": "Your bills: Gas 29 322 so'm. Pay?" }
```

`fallback` is plain text for clients that can draw nothing. The server builds it from the `alt` of every component in tree order.

### `ui.patch`

```json
{ "v": 1, "t": "ui.patch", "surface": "s1", "seq": 2, "upsert": [], "remove": ["old_note"], "root": "card" }
```

`upsert` replaces whole components by id or adds new ones. `remove` deletes by id and also deletes the removed ids from every `children` list. `root` is optional.

### `ui.data`

```json
{ "v": 1, "t": "ui.data", "surface": "s1", "seq": 3, "ops": [ { "op": "set", "path": "/bills/0/paid", "value": true } ] }
```

Operations, applied in order:

| `op` | Effect |
|---|---|
| `set` | Put `value` at `path`, creating missing objects on the way. |
| `merge` | JSON Merge Patch (RFC 7396) of `value` into the object at `path`. |
| `remove` | Delete `path`. Missing paths are fine. |

### `ui.close`

```json
{ "v": 1, "t": "ui.close", "surface": "s1", "seq": 4 }
```

### `ui.sync`

Only sent to clients with the `sync` feature.

```json
{ "v": 1, "t": "ui.sync", "surface": "s1", "highlight": "opt_2", "at_ms": 1840 }
```

While the voice layer speaks, the client highlights the component being spoken. `at_ms` is the offset into the current utterance. `highlight: null` clears it.

### `ui.snapshot`

The full state of a surface: `root`, `components`, `data`, `seq`. Sent after `ui.resync` and when a client reconnects.

## 9. Client to server messages

| `t` | Fields | When |
|---|---|---|
| `hello` | section 4 | First message. |
| `ui.action` | section 7 | User tapped, typed or spoke. |
| `ui.resync` | `surface` | Client saw a `seq` gap or lost state. |
| `ui.error` | `surface`, `reason`, `detail` | Client could not draw something. Reasons: `unsupported_version`, `unknown_component`, `render_failed`, `bad_message`. |

The transport is not part of the protocol. The reference server speaks WebSocket (both directions on one connection) and Server-Sent Events plus HTTP POST (server messages on the event stream, client messages as POST bodies). Both carry the same JSON.

## 10. What the model sees

The model calls tools. The server turns valid calls into the messages above.

### Tools

| Tool | Input |
|---|---|
| `show_ui` | `{ "surface"?: string, "root": string, "components": [...], "data"?: object }` |
| `update_ui` | `{ "surface": string, "upsert"?: [...], "remove"?: [string], "root"?: string, "data"?: [ops] }` |
| `close_ui` | `{ "surface": string }` |

The server generates the JSON Schema for these tools from the catalogs the client declared, so the model only sees types the client can draw. The schemas avoid JSON Schema features that constrained decoders do not support (`not`, `if/then/else`, `uniqueItems`, `contains`, `propertyNames`).

In tool input, `data` is not a free-form object. It is a list of `{ "path": "/bills/0/amount", "value": ... }` entries where `value` is a string, number or bool. The server builds the data model from them. This keeps every object in the schema closed (`additionalProperties: false`), which strict tool use and grammar-based decoders need, and it is easier for small models than writing nested JSON.

There is a strict mode for providers that enforce the schema while decoding (for example `strict: true` on Anthropic tools). In strict mode the schema leaves out length and range limits, because those providers do not support them, and every property of every object is listed in `required`. A property that may be left out accepts `null` instead (an `enum` gets `null` as one more value, a boolean becomes `{"enum": [true, false, null]}`, anything else becomes `anyOf` with `{"type": "null"}`). Lists that may be left out (`children`, `with`, `data`, `upsert`, `remove`) stay required and take `[]` for none. The validator treats `null` in any object field exactly like a missing field, in both modes. The server still checks every limit after the call.

Providers also limit how complex strict schemas may be. The Anthropic API allows at most 20 strict tools, 24 optional parameters and 16 parameters with union types (`anyOf` or type arrays) across all strict tools in one request. A tool is marked `strict` only when its strict schema fits what is left of these limits; the others are sent in the normal form. With the built-in catalogs every component prop that accepts `$bind` or `$ref` is a union, so `show_ui` and `update_ui` exceed the union limit (the API counted 116 union parameters for the two) and are sent without `strict`, while `close_ui` is strict.

### Tool results

On success the tool result is a short summary of what is now on screen, for example:

```
surface s1 open: Card "Your bills" > List(2) > [Button "Gas 29 322 so'm" (act), Button "Power [ref]" (act)]
```

On failure it is a numbered list of problems with the component id and prop, for example:

```
2 problems, nothing was shown:
1. components[3] id "pay": type "PayButton" is not in the catalog. Known: Button, ChoiceGroup, ...
2. components[1] id "list": children references "row9" which does not exist.
```

When the user acts, the next model turn gets the action and the summary of the surface at that moment, so "the second one" refers to the right thing.

## 11. Voice

Every catalog type has a `say` template. A voice-only client gets the same messages as a screen client and turns each surface into speech in tree order. Choices are read with their ordinal ("first, Gas; second, Power").

When the user speaks, the client matches the words against the surface before sending anything to the model. A match produces a normal `ui.action` with `source: "voice"`. The matcher understands:

- ordinals in the client's locale ("ikkinchisi", "второй", "the second one"),
- labels and `voice.synonyms`,
- yes and no for confirm sheets.

A yes or no for a confirm sheet must be the whole utterance, at most 6 words: yes or no words plus polite fillers ("yes please", "ha, tasdiqlayman", "да, подтверждаю"). A negation anywhere ("not", "don't", "не", "emas", an Uzbek verb in the negative form such as "tasdiqlamayman"), a word that is neither, or an utterance with both a yes and a no word is never a yes. A negation with no yes word is a no. Anything else is not an answer, and the confirm sheet stays open.

Words that match nothing go to the model as ordinary user speech.

If the user talks over the assistant (barge-in), the client stops speech and keeps the surface. The model decides whether to patch or close it.

## 12. Versioning

- Protocol version `v` changes only for breaking changes to messages.
- Catalogs have their own major version, `core@1`. A new optional prop is not breaking. Removing a prop or a type is.
- A renderer that gets a component type it does not know draws the component's `alt` as text and sends `ui.error` with reason `unknown_component` once per type.

## 13. Security rules

1. The model never produces markup, script, CSS or URLs that are executed. It only names catalog types and fills typed props.
2. Props of kind `url` accept `https` (and `http` only when the host allows it). `javascript:`, `data:`, `file:` and everything else are rejected by the server and again by the renderer.
3. Style is chosen from enums (`tone`, `size`, `variant`). There is no free-form style prop.
4. Invalid output never reaches a client.
5. `$ref` values are resolved on the server, per user, and never enter the model's context.
6. Actions run on the server, from handles, with server-built confirm text when the host marks them risky.
7. Limits in section 5.2 are checked before full parsing.

## 14. Chat extras used by the reference server

These are not part of the UI protocol. The reference server and the example chat client use them so one connection can carry the whole conversation. Other hosts can use their own chat transport.

| Direction | Message | Meaning |
|---|---|---|
| client to server | `{ "v": 1, "t": "user.say", "text": "..." }` | The user typed or said something to the agent. |
| server to client | `{ "v": 1, "t": "agent.delta", "text": "..." }` | A piece of the agent's text answer, streamed. |
| server to client | `{ "v": 1, "t": "agent.done" }` | The agent finished its turn. |
| server to client | `{ "v": 1, "t": "agent.error", "message": "..." }` | The turn failed. The message is safe to show to the user. |

The WebSocket endpoint is `/ws`. The client sends `hello` first and waits for `welcome`.
