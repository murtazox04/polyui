Run on 2026-11-24 with `claude-haiku-5-5` and the compact tool profile: 32 prompts, 63 model calls, about $0.0325.

| Setting | Turns with a UI call | Valid on the first try | Valid after 1 retry | Valid after 2 retries | UI shown when expected | No UI when none expected | Mean time to ui.open | Model calls | Input tokens | Output tokens | Cost |
|---|---|---|---|---|---|---|---|---|---|---|---|
| tool calls | 26 of 32 | 100% (26/26) | 100% (26/26) | 100% (26/26) | 92% (22/24) | 100% (4/4) | 4.5 s | 63 | 271223 | 27574 | $0.0325 |

| Locale | Setting | Turns with a UI call | Valid on the first try | Valid after 2 retries | Mean time to ui.open |
|---|---|---|---|---|---|
| en | tool calls | 6 of 8 | 100% (6/6) | 100% (6/6) | 4.3 s |
| uz-Latn | tool calls | 7 of 8 | 100% (7/7) | 100% (7/7) | 4.3 s |
| uz-Cyrl | tool calls | 7 of 8 | 100% (7/7) | 100% (7/7) | 4.5 s |
| ru | tool calls | 6 of 8 | 100% (6/6) | 100% (6/6) | 4.9 s |

| Setting | Most common problems | Count |
|---|---|---|
| tool calls | none | 0 |
