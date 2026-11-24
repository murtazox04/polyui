# How often the model gets the UI right

PolyUI only works if the model writes a `show_ui` call that passes validation. When it does not, the server sends the problems back and the model retries, which costs time and tokens. This page explains how we measure that, what the latest run showed, and how to run it yourself.

The short version for the run on 2026-11-24 with `claude-haiku-5-5`: every UI the model attempted was valid on the first try, in all four languages, in both settings we compared. The two settings differed in speed, cost and in what the model chose to draw, not in validity.

## What is measured

The `polyui-bench` binary (crate `bench/`) sends 32 realistic requests to the real Anthropic API through `AnthropicProvider`, against the `bills` demo host (the Karimov household in Tashkent). Each request runs in a fresh session, so earlier answers and paid bills never leak into the next one.

For every turn it records:

- each UI attempt in order, and whether the server accepted it,
- the time from sending the message to the first `ui.open`,
- the number of model calls, and input, cache and output tokens,
- the problem lines of every rejected attempt, normalized into kinds (quoted names become `"x"`, numbers become `N`), so they can be counted.

From that it reports, per locale and overall:

| Number | Meaning |
|---|---|
| Valid on the first try | Of the turns where the model tried to draw UI, the share whose first attempt was accepted. |
| Valid after 1 or 2 retries | The share with an accepted attempt by the second or third try. The server gives up after two failed retries. |
| UI shown when expected | Of the requests that clearly need UI, the share where a `ui.open` reached the client. |
| No UI when none expected | Of the requests that need no UI (a thank you, a definition), the share where the model did not draw any. |
| Mean time to ui.open | Wall time from the user message to the first `ui.open`, over turns that opened one. |
| Cost | Input tokens at $0.10 per million, output at $0.50 per million. Cache writes are billed at 1.25 times the input price and cache reads at 0.1 times, as Anthropic documents for 5 minute cache entries. |

## The two settings

The question behind the comparison: does asking for UI through tool calls beat asking the model to write the same JSON inline?

```mermaid
flowchart LR
    U[User message] --> A
    U --> B
    subgraph A [Tool calls, as the server works today]
        A1[Model gets show_ui, update_ui, close_ui and get_usage as tools] --> A2[show_ui tool call]
        A2 --> A3[Server validates]
        A3 -- problems --> A1
        A3 -- valid --> A4[ui.open, then the model writes a closing line]
    end
    subgraph B [Inline JSON]
        B1[Model gets get_usage as a tool and the show_ui schema in the system prompt] --> B2[show_ui JSON in a fenced json block]
        B2 --> B3[Same validator]
        B3 -- problems --> B1
        B3 -- valid --> B4[ui.open, turn ends]
    end
```

**Tool calls** is the real agent loop in `polyui-server`, unchanged: the same system prompt, the same tool list (non-strict `show_ui` and `update_ui`, strict `close_ui` and `get_usage`), the same retry limit.

**Inline JSON** uses the same session, host and validator, but the UI tools are taken out of the tool list. The system prompt gets one more paragraph that says to write the `show_ui` input as JSON in a fenced block, followed by the `show_ui` input schema. The benchmark pulls the first fenced block that starts with `{`, passes it to `Session::show_ui`, and on problems sends them back as a user message, with the same limit of two retries.

Two differences are built into the comparison and you should keep them in mind when reading the numbers:

- The inline prompt carries only the `show_ui` schema, while the tool setting sends all three UI tool schemas. That is the main reason its input is smaller.
- With tools, the model writes its text after it sees that the UI was shown, which takes a second model call. Inline, the text and the JSON come in one reply.

## The prompt set

Eight kinds of request in four locales: English, Uzbek in Latin script, Uzbek in Cyrillic, and Russian. The full text is in `bench/src/prompts.rs`.

| Kind | Example (English) | Expected |
|---|---|---|
| bills | What do I owe this month? Show me all my bills. | UI |
| pay | Pay my gas bill. (the confirm step is the host's, not the model's) | UI |
| offers | Compare the internet tariffs and let me pick one. | UI |
| chart | Show a chart of my electricity usage over the last six months. | UI |
| phone-form | I want to change the phone number for payment notifications. Ask me for the new number. | UI |
| bill-detail | How much is my internet bill and when is it due? | UI |
| ambiguous | the water thing | either |
| no-ui | What does kWh stand for? | no UI |

## Results, 2026-11-24, `claude-haiku-5-5`

32 prompts, two settings, 102 model calls, about $0.16. The raw data, including every attempt and the first 400 characters of each answer, is in `bench/results/2026-11-24-claude-haiku-5-5.json`. A 4 prompt smoke run before it used 14 more calls (about $0.02).

| Setting | Turns with a UI call | Valid on the first try | Valid after 1 retry | Valid after 2 retries | UI shown when expected | No UI when none expected | Mean time to ui.open | Model calls | Input tokens | Output tokens | Cost |
|---|---|---|---|---|---|---|---|---|---|---|---|
| tool calls | 28 of 32 | 100% (28/28) | 100% (28/28) | 100% (28/28) | 100% (24/24) | 100% (4/4) | 5.1 s | 65 | 1443493 | 30368 | $0.1037 |
| inline JSON | 24 of 32 | 100% (24/24) | 100% (24/24) | 100% (24/24) | 83% (20/24) | 100% (4/4) | 6.4 s | 37 | 393644 | 34027 | $0.0553 |

Input tokens include cache writes and cache reads. For tool calls that is 194 uncached, 643 835 written to the cache and 799 464 read from it; for inline JSON 138, 298 215 and 95 291.

| Locale | Setting | Turns with a UI call | Valid on the first try | Valid after 2 retries | Mean time to ui.open |
|---|---|---|---|---|---|
| en | tool calls | 7 of 8 | 100% (7/7) | 100% (7/7) | 3.9 s |
| en | inline JSON | 6 of 8 | 100% (6/6) | 100% (6/6) | 5.5 s |
| uz-Latn | tool calls | 7 of 8 | 100% (7/7) | 100% (7/7) | 4.8 s |
| uz-Latn | inline JSON | 6 of 8 | 100% (6/6) | 100% (6/6) | 6.6 s |
| uz-Cyrl | tool calls | 7 of 8 | 100% (7/7) | 100% (7/7) | 6.3 s |
| uz-Cyrl | inline JSON | 6 of 8 | 100% (6/6) | 100% (6/6) | 7.2 s |
| ru | tool calls | 7 of 8 | 100% (7/7) | 100% (7/7) | 5.4 s |
| ru | inline JSON | 6 of 8 | 100% (6/6) | 100% (6/6) | 6.2 s |

| Setting | Most common problems | Count |
|---|---|---|
| tool calls | none | 0 |
| inline JSON | none | 0 |

Time to the first `ui.open`, mean per locale, in seconds:

```mermaid
xychart-beta
    title "Mean time to ui.open (s), line 1: tool calls, line 2: inline JSON"
    x-axis [en, uz-Latn, uz-Cyrl, ru]
    y-axis "seconds" 0 --> 8
    line [3.9, 4.8, 6.3, 5.4]
    line [5.5, 6.6, 7.2, 6.2]
```

## What the run tells us

**Validity is not where the settings differ.** No attempt was rejected in either setting, so this run does not support the idea that tool calling produces more valid UI than inline JSON. With every rate at 100%, the benchmark cannot rank them on validity at all; a harder prompt set or a smaller model would be needed for that.

**The model draws a form only with tools.** All four phone number requests opened a surface in the tool setting, and the text around it pointed the user to the field on screen. Inline, all four got a plain text question instead ("What's the new phone number...?") and no JSON block. That is the whole gap in "UI shown when expected", 20 of 24 against 24 of 24. One run per prompt is not enough to call this a rule, but it was consistent across all four languages.

**Tools were faster to first paint.** The mean time to `ui.open` was 5.1 s with tools and 6.4 s inline, and tools were ahead in every locale. The medians tell the same story: 4.2 s against 6.0 s. In both settings the server acts only after the model's reply is complete (the agent loop runs tool calls after `message_stop`), so the gap is in how long the first reply takes to generate. We did not measure which part of that reply takes the time.

**Inline was about half the price.** It used 37 model calls against 65 and cost $0.055 against $0.104. Most of that is the closing call the tool loop makes after a successful `show_ui`, and the larger tool schemas. The tool setting relied heavily on the prompt cache: 55% of its input tokens were cache reads.

**Uzbek in Cyrillic was the slowest locale in both settings.** It also had the slowest single turn of the run, the bills request with tools at 16.7 s end to end. We did not measure tokens per character by script, so we cannot say how much of this is tokenization.

Limits of this run: one sample per prompt and setting, 32 prompts, one model, and a single day. Treat differences of a few percent or a few hundred milliseconds as noise.

## Running it

The key comes from an env file and is never printed.

```sh
set -a; . ~/.config/polyui/anthropic.env; set +a
cargo run --release -p polyui-bench -- --limit 4 --max-calls 24 --out-dir /tmp/polyui-bench --name smoke
cargo run --release -p polyui-bench -- --max-calls 160
```

| Option | Default | Use |
|---|---|---|
| `--model` | `claude-haiku-5-5` | Any model id the Messages API accepts. |
| `--variant` | `both` | `tools`, `inline` or `both`. |
| `--limit` | all 32 | Run only the first N prompts. The first four cover four locales and four kinds of request. |
| `--max-calls` | 160 | Hard cap on model calls for the whole run. Calls over the cap fail and the turn is marked `budget`. |
| `--concurrency` | 4 | Turns in flight at once. |
| `--env-file` | none | Read `ANTHROPIC_API_KEY` from this file instead of the environment. |
| `--input-price`, `--output-price` | 0.10, 0.50 | Dollars per million tokens. |
| `--list` | off | Print the prompt set and exit, without calling the model. |

Each run writes `<date>-<model>.json` and `<date>-<model>.md` to `--out-dir` (default `bench/results`). `cargo test -p polyui-bench` checks the scoring, the prompt set and the inline parser with scripted replies and never calls the network.
