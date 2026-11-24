Run on 2026-11-24 with `claude-haiku-5-5` and the compact tool profile: 8 prompts, 16 model calls, about $0.0093.

| Setting | Turns with a UI call | Valid on the first try | Valid after 1 retry | Valid after 2 retries | UI shown when expected | No UI when none expected | Mean time to ui.open | Model calls | Input tokens | Output tokens | Cost |
|---|---|---|---|---|---|---|---|---|---|---|---|
| tool calls | 6 of 8 | 100% (6/6) | 100% (6/6) | 100% (6/6) | 83% (5/6) | 100% (1/1) | 5.4 s | 16 | 71543 | 7965 | $0.0093 |

| Locale | Setting | Turns with a UI call | Valid on the first try | Valid after 2 retries | Mean time to ui.open |
|---|---|---|---|---|---|
| en | tool calls | 1 of 2 | 100% (1/1) | 100% (1/1) | 6.4 s |
| uz-Latn | tool calls | 2 of 2 | 100% (2/2) | 100% (2/2) | 4.0 s |
| uz-Cyrl | tool calls | 2 of 2 | 100% (2/2) | 100% (2/2) | 5.2 s |
| ru | tool calls | 1 of 2 | 100% (1/1) | 100% (1/1) | 7.5 s |

| Setting | Most common problems | Count |
|---|---|---|
| tool calls | none | 0 |
