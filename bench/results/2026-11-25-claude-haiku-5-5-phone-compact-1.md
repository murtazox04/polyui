Run on 2026-11-25 with `claude-haiku-5-5` and the compact tool profile: 4 prompts, 10 model calls, about $0.0043.

| Setting | Turns with a UI call | Valid on the first try | Valid after 1 retry | Valid after 2 retries | UI shown when expected | No UI when none expected | Mean time to ui.open | Model calls | Input tokens | Output tokens | Cost |
|---|---|---|---|---|---|---|---|---|---|---|---|
| tool calls | 4 of 4 | 50% (2/4) | 75% (3/4) | 100% (4/4) | 100% (4/4) | n/a | 4.0 s | 10 | 43675 | 3162 | $0.0043 |

| Locale | Setting | Turns with a UI call | Valid on the first try | Valid after 2 retries | Mean time to ui.open |
|---|---|---|---|---|---|
| en | tool calls | 1 of 1 | 0% (0/1) | 100% (1/1) | 4.4 s |
| uz-Latn | tool calls | 1 of 1 | 0% (0/1) | 100% (1/1) | 5.0 s |
| uz-Cyrl | tool calls | 1 of 1 | 100% (1/1) | 100% (1/1) | 3.2 s |
| ru | tool calls | 1 of 1 | 100% (1/1) | 100% (1/1) | 3.4 s |

| Setting | Most common problems | Count |
|---|---|---|
| tool calls | $bind and $ref are not allowed here | 3 |
