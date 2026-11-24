# The catalogs

PolyUI ships three catalogs. This page lists every type, prop, limit and default in them. It is generated from the JSON files in [`spec/catalogs`](https://github.com/murtazox04/polyui/tree/main/spec/catalogs), which are the source of truth; the [catalog format](../spec/catalog.md) explains the fields.

A few rules apply to every type:

- Props marked required must be present. Every other prop may be left out, and a default is filled in by the client core when there is one.
- Any prop except actions, paths and the `embed:Embed` resource may be `{"$bind": "/path"}` (read from the data model) or `{"$ref": "source:key"}` (filled in on the server). See [Concepts](concepts.md#the-data-model-and-bind).
- Unknown props are an error. There is no free-form style prop; look comes from enums such as `tone` and `style`.
- `alt` is the text form used when a client cannot draw a type, and in the fallback text of every `ui.open`. `say` is the spoken form. `{children}` joins the children's forms in order, `{options|ordinals}` reads options as "first, a; second, b".

## `core@1`

### `Text`

A paragraph or heading of plain text.

Role `text`. Children: none. Alt: `{text}`. Say: `{text}`.

| Prop | Required | Accepts |
|---|---|---|
| `text` | yes | text, up to 4000 characters |
| `style` |  | one of `body`, `title`, `subtitle`, `caption`, `muted`, default `body` |

### `Card`

A box that groups related content under an optional title.

Role `group`. Children: any type. Alt: `{title}. {children}`. Say: `{title}. {children}`.

| Prop | Required | Accepts |
|---|---|---|
| `title` |  | text, up to 120 characters |
| `subtitle` |  | text, up to 200 characters |
| `tone` |  | one of `plain`, `info`, `success`, `warning`, `danger`, default `plain` |

### `Row`

Places children side by side, wrapping on narrow screens.

Role `group`. Children: any type. Alt: `{children}`. Say: `{children}`.

| Prop | Required | Accepts |
|---|---|---|
| `gap` |  | one of `none`, `small`, `medium`, `large`, default `medium` |
| `align` |  | one of `start`, `center`, `end`, `between`, default `start` |

### `List`

A vertical list. Each child is one item.

Role `list`. Children: any type. Alt: `{title}: {children}`. Say: `{title}. {children}`.

| Prop | Required | Accepts |
|---|---|---|
| `title` |  | text, up to 120 characters |
| `ordered` |  | `true` or `false`, default `false` |

### `Stat`

One important number with a label, like a balance or a total.

Role `status`. Children: none. Alt: `{label}: {value} {unit}`. Say: `{label} {value} {unit}`.

| Prop | Required | Accepts |
|---|---|---|
| `label` | yes | text, up to 80 characters |
| `value` | yes | text, up to 40 characters |
| `unit` |  | text, up to 16 characters |
| `tone` |  | one of `neutral`, `positive`, `negative`, `warning`, default `neutral` |
| `hint` |  | text, up to 120 characters |

### `Badge`

A short status label.

Role `status`. Children: none. Alt: `({text})`. Say: `{text}`.

| Prop | Required | Accepts |
|---|---|---|
| `text` | yes | text, up to 32 characters |
| `tone` |  | one of `neutral`, `info`, `success`, `warning`, `danger`, default `neutral` |

### `Image`

A picture from an https URL. Always give alt text.

Role `img`. Children: none. Alt: `[image: {alt}]`. Say: `{alt}`.

| Prop | Required | Accepts |
|---|---|---|
| `src` | yes | URL, scheme `https` |
| `alt` | yes | text, up to 200 characters |
| `aspect` |  | one of `square`, `wide`, `tall`, `auto`, default `auto` |

### `Button`

A tappable button that fires an action.

Role `button`. Children: none. Alt: `[{label}]`. Say: `{label}`.

| Prop | Required | Accepts |
|---|---|---|
| `label` | yes | text, up to 80 characters |
| `tone` |  | one of `primary`, `secondary`, `danger`, default `secondary` |
| `on_press` | yes | action |
| `disabled` |  | `true` or `false`, default `false` |

### `ChoiceGroup`

Lets the user pick one or more options. Use for short lists of offers, stores or answers.

Role `radiogroup`. Children: none. Alt: `{label}: {options|list}`. Say: `{label}. {options|ordinals}`.

| Prop | Required | Accepts |
|---|---|---|
| `label` |  | text, up to 120 characters |
| `options` | yes | list of objects with `id`, `label`, `detail`, at most 12 items |
| `multiple` |  | `true` or `false`, default `false` |
| `value` | yes | data model path |
| `on_change` |  | action |

### `Input`

A single text field that writes into the data model.

Role `textbox`. Children: none. Alt: `{label}: ____`. Say: `{label}?`.

| Prop | Required | Accepts |
|---|---|---|
| `label` | yes | text, up to 80 characters |
| `value` | yes | data model path |
| `input` |  | one of `text`, `number`, `phone`, `email`, `multiline`, default `text` |
| `placeholder` |  | text, up to 80 characters |
| `max_length` |  | whole number, 1 to 4000, default `200` |
| `required` |  | `true` or `false`, default `false` |

### `ConfirmSheet`

Asks the user a yes or no question. For risky actions the host adds its own confirmation, so use this only for simple questions.

Role `dialog`. Children: none. Alt: `{title} {body} [{confirm_label}] [{cancel_label}]`. Say: `{title}. {body}`.

| Prop | Required | Accepts |
|---|---|---|
| `title` | yes | text, up to 120 characters |
| `body` |  | text, up to 400 characters |
| `confirm_label` |  | text, up to 40 characters, default `Yes` |
| `cancel_label` |  | text, up to 40 characters, default `No` |
| `on_confirm` | yes | action |
| `on_cancel` |  | action |

### `Divider`

A thin line between sections.

Role `group`. Children: none. Alt: ``. Say: ``.

## `chart@1`

### `chart:Chart`

A bar, line, area, pie or scatter chart drawn from rows of data.

Role `figure`. Children: none. Alt: `{title}`. Say: `{title}`.

| Prop | Required | Accepts |
|---|---|---|
| `title` | yes | text, up to 120 characters |
| `kind` | yes | one of `bar`, `line`, `area`, `pie`, `scatter` |
| `x_label` |  | text, up to 64 characters |
| `y_label` |  | text, up to 64 characters |
| `unit` |  | text, up to 16 characters |
| `rows` | yes | list of objects with `x`, `y`, `series`, at most 1000 items |
| `on_select` |  | action |

### `chart:Dashboard`

A grid of charts and stats.

Role `group`. Children: `chart:Chart`, `Stat`, `Card`, `Text`. Alt: `{title}. {children}`. Say: `{title}. {children}`.

| Prop | Required | Accepts |
|---|---|---|
| `title` |  | text, up to 120 characters |
| `columns` |  | whole number, 1 to 4, default `2` |

## `embed@1`

### `embed:Embed`

Shows an MCP App the host provides, in a sandboxed frame. Use only when no other component can show it.

Role `figure`. Children: none. Alt: `{title}`. Say: `{title}`.

| Prop | Required | Accepts |
|---|---|---|
| `resource` | yes | URL, scheme `ui`, not bindable |
| `title` | yes | text, up to 120 characters |
| `height` |  | one of `small`, `medium`, `large`, default `medium` |
| `on_message` |  | action |

