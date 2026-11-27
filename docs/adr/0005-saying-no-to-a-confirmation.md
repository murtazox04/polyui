# 5. Saying no to a confirmation

Date: 2026-11-27
Status: accepted

## What we need to decide

A confirmation (`ui.confirm`, section 7.2 of the protocol) has two ways out today: the person says yes and the client sends the action again with the nonce, or the question expires on the server. Cancel in the renderer's sheet closes the sheet and sends nothing. For an action that is only untidy: the server keeps a pending nonce until it expires. For an app call that the host asked about ([ADR 4](0004-confirming-an-app-call.md)) it is visible: the app shows "Waiting for your confirmation" for up to 30 seconds after the person already said no, and only one app question may be open per session, so the next one is refused for that long too.

## Proposal

1. **Opt in.** A client that sends a no adds `dismiss` to `features` in `hello`. The server lists it in `welcome.features` when it accepts it, and a client sends the new message only then, so an older server never sees an unknown message.
2. **The message.** When the person cancels a confirm sheet, the client sends `{ "v": 1, "t": "ui.dismiss", "surface": "s1", "action_id": "act_4kq9", "nonce": "n_77ad" }`, with the `action_id` and nonce of the `ui.confirm` it showed. Sheets the renderer opens for its own questions, such as the link question for an app, send nothing.
3. **On the server.** A `ui.dismiss` that matches an open question (same surface, `action_id` and nonce, the nonce compared in constant time) ends it at once and is answered with `ui.action.result` `rejected` and the message "Cancelled.", which also clears the question in the client core. For an app question the app also gets `app.result` with `-32000` "The person said no." Anything that does not match is ignored without an answer, so a late or repeated no changes nothing and tells the sender nothing.
4. **The model.** A no to an action does not start a turn, as a cancel does not today. The model sees the next thing the person says or taps.

## Failure modes

| Risk | What could go wrong | What limits it |
|---|---|---|
| A forged no | Someone ends another person's question. | The nonce is needed, and it only ever went to the client that showed the sheet. |
| A no for a question that was already answered | A yes and a no cross on the wire. | The first one wins; the second matches nothing and is ignored. |
| An older client | It never sends `ui.dismiss`. | Questions expire as before. |

## Alternatives

| Option | Why not |
|---|---|
| `ui.action` with `confirm` and a `decline` flag | Mixes a yes and a no in one message that already runs things; a bug that drops the flag would turn a no into a yes. |
| Let the app question time out, as now | The app waits 30 seconds for an answer the person already gave. |

## What changes

- `spec/protocol.md` section 7.2 and the client message table.
- `polyui-server`: `ui.dismiss` for action confirmations and app questions, with tests.
- Renderers on web, iOS, Android and Flutter send `ui.dismiss` from the Cancel of the confirm sheet when the server accepted `dismiss`, and the examples declare it.
