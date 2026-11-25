# Catalog format

A catalog is one JSON file. It lists component types and the rules for their props. The server uses it to check model output and to generate tool schemas. Renderers use it to know what they must draw. The same file is the source of truth in every language, so there is nothing to keep in sync by hand.

PolyUI ships three catalogs: [`core`](https://github.com/murtazox04/polyui/blob/main/crates/polyui-core/catalogs/core.json), [`chart`](https://github.com/murtazox04/polyui/blob/main/crates/polyui-core/catalogs/chart.json) and [`embed`](https://github.com/murtazox04/polyui/blob/main/crates/polyui-core/catalogs/embed.json). `embed` is off unless the host turns it on (see [Embedding MCP Apps](../docs/embed.md)), so the catalogs offered to a model by default are `core` and `chart`. Hosts add their own with a namespace, for example `acme`.

## Shape

```json
{
  "id": "core",
  "version": 1,
  "types": {
    "Button": {
      "description": "A tappable button that fires an action.",
      "role": "button",
      "children": false,
      "props": {
        "label": { "kind": "string", "max": 80, "required": true },
        "tone": { "kind": "enum", "values": ["primary", "secondary", "danger"], "default": "secondary" },
        "on_press": { "kind": "action", "required": true }
      },
      "alt": "[{label}]",
      "say": "{label}"
    }
  }
}
```

| Field | Meaning |
|---|---|
| `id` | Catalog id. `core`, `chart` and `embed` are reserved. Extension ids are lowercase letters and digits. |
| `version` | Major version. Bump it only for breaking changes. |
| `types` | Map from type name to definition. Names are `PascalCase`. Extension types are referenced as `id:Name`. |
| `description` | One sentence shown to the model in the tool schema. Keep it short, small models read every word. |
| `role` | Accessibility role the renderer must expose: `text`, `group`, `list`, `listitem`, `button`, `radiogroup`, `img`, `textbox`, `dialog`, `figure`, `status`. |
| `children` | `false`, `true` (any type) or a list of allowed type names. |
| `props` | Map from prop name to prop spec. Prop names are `snake_case`. |
| `alt` | Template for the text fallback. |
| `say` | Template for the spoken form. |

## Prop kinds

| `kind` | Extra fields | Accepts |
|---|---|---|
| `string` | `max`, `min` | A string. `max` and `min` count characters (Unicode code points), not bytes. |
| `int` | `min`, `max` | A whole number. |
| `number` | `min`, `max` | Any finite number. |
| `bool` | | `true` or `false`. |
| `scalar` | | A string, a finite number, or a bool. |
| `enum` | `values` | One of `values`. |
| `url` | `schemes` (default `["https"]`) | An absolute URL with an allowed scheme. A `ui` URL must start with `ui://` followed by a name. |
| `action` | | An action object (protocol section 7). |
| `path` | | A JSON Pointer into the surface data model. Used by inputs to say where they write. |
| `array` | `of` (a prop spec), `max` | A list. |
| `object` | `fields` (map of prop specs) | An object with exactly these fields. |

Every spec can also have:

- `required`: the prop must be present. Default `false`.
- `default`: value used when the prop is missing.
- `bindable`: whether `{"$bind": ...}` and `{"$ref": ...}` are allowed. Default `true` for every kind except `action` and `path`.
- `description`: a short hint for the model.

A string field named `label` inside an `object` prop (for example a ChoiceGroup option) must not be blank or only whitespace, whether it is written directly or comes from `$bind` or `$ref`. Voice matching numbers options by position, so every option needs a label to read.

## Templates

`alt` and `say` are strings with placeholders.

| Placeholder | Becomes |
|---|---|
| `{name}` | The prop value as text. Missing props become an empty string. |
| `{name|list}` | An array prop joined as "a, b and c" in the client's locale. |
| `{name|ordinals}` | An array prop read with ordinals: "first, a; second, b". Items that are objects use their `label` field. Each item keeps its position, so an empty item is skipped but does not shift the numbers after it. |
| `{children}` | The `alt` (or `say`) of every child, in order, joined with ". ". |

Ordinal words and list joiners come from the locale table in `polyui-core`. Version 1 ships `en`, `uz-Latn`, `uz-Cyrl` and `ru`.
