Run on 2026-11-24 with `model/chat/ember` and the compact tool profile: 8 prompts, 17 model calls, about $0.0000. 17 HTTP requests including retries.

| Setting | Turns with a UI call | Valid on the first try | Valid after 1 retry | Valid after 2 retries | UI shown when expected | No UI when none expected | Mean time to ui.open | Model calls | Input tokens | Output tokens | Cost |
|---|---|---|---|---|---|---|---|---|---|---|---|
| inline JSON | 5 of 8 | 20% (1/5) | 20% (1/5) | 20% (1/5) | 17% (1/6) | 100% (1/1) | 69.0 s | 17 | 42261 | 2924 | $0.0000 |

| Locale | Setting | Turns with a UI call | Valid on the first try | Valid after 2 retries | Mean time to ui.open |
|---|---|---|---|---|---|
| en | inline JSON | 1 of 2 | 0% (0/1) | 0% (0/1) | n/a |
| uz-Latn | inline JSON | 2 of 2 | 50% (1/2) | 50% (1/2) | 69.0 s |
| uz-Cyrl | inline JSON | 2 of 2 | 0% (0/2) | 0% (0/2) | n/a |
| ru | inline JSON | 0 of 2 | n/a | n/a | n/a |

| Setting | Most common problems | Count |
|---|---|---|
| inline JSON | the input is not valid JSON | 9 |
| inline JSON | is not reachable from root "x" | 6 |
| inline JSON | action "x" is not registered | 2 |
| inline JSON | ConfirmSheet cannot have children | 1 |
| inline JSON | required prop is missing | 1 |
