Run on 2026-11-24 with `model/chat/ember` and the compact tool profile: 8 prompts, 9 model calls, about $0.0000. 9 HTTP requests including retries. The run stopped early after provider errors in a row.

| Setting | Turns with a UI call | Valid on the first try | Valid after 1 retry | Valid after 2 retries | UI shown when expected | No UI when none expected | Mean time to ui.open | Model calls | Input tokens | Output tokens | Cost |
|---|---|---|---|---|---|---|---|---|---|---|---|
| constrained JSON | 2 of 8 | 0% (0/2) | 0% (0/2) | 0% (0/2) | 0% (0/6) | 100% (1/1) | n/a | 9 | 16580 | 927 | $0.0000 |

| Locale | Setting | Turns with a UI call | Valid on the first try | Valid after 2 retries | Mean time to ui.open |
|---|---|---|---|---|---|
| en | constrained JSON | 1 of 2 | 0% (0/1) | 0% (0/1) | n/a |
| uz-Latn | constrained JSON | 0 of 2 | n/a | n/a | n/a |
| uz-Cyrl | constrained JSON | 1 of 2 | 0% (0/1) | 0% (0/1) | n/a |
| ru | constrained JSON | 0 of 2 | n/a | n/a | n/a |

| Setting | Most common problems | Count |
|---|---|---|
| constrained JSON | is not reachable from root "x" | 2 |
| constrained JSON | required prop is missing | 2 |
| constrained JSON | unknown field "x" | 2 |
| constrained JSON | "x","x":[{"x":"x","x":"x","x":{"x":"x" is not in components | 1 |
| constrained JSON | Text cannot have children | 1 |
| constrained JSON | components is required | 1 |
