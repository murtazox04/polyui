Run on 2026-11-25 with `claude-haiku-5-5` and the compact tool profile: 32 prompts, 68 model calls, about $0.0349.

| Setting | Turns with a UI call | Valid on the first try | Valid after 1 retry | Valid after 2 retries | UI shown when expected | No UI when none expected | Mean time to ui.open | Model calls | Input tokens | Output tokens | Cost |
|---|---|---|---|---|---|---|---|---|---|---|---|
| tool calls | 28 of 32 | 96% (27/28) | 100% (28/28) | 100% (28/28) | 100% (24/24) | 100% (4/4) | 4.6 s | 68 | 302472 | 30353 | $0.0349 |

| Locale | Setting | Turns with a UI call | Valid on the first try | Valid after 2 retries | Mean time to ui.open |
|---|---|---|---|---|---|
| en | tool calls | 7 of 8 | 100% (7/7) | 100% (7/7) | 3.7 s |
| uz-Latn | tool calls | 7 of 8 | 100% (7/7) | 100% (7/7) | 4.3 s |
| uz-Cyrl | tool calls | 7 of 8 | 86% (6/7) | 100% (7/7) | 6.2 s |
| ru | tool calls | 7 of 8 | 100% (7/7) | 100% (7/7) | 4.2 s |

| Setting | Most common problems | Count |
|---|---|---|
| tool calls | children references "x" which already has parent "x" | 1 |
