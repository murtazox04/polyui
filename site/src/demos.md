# Live demos

These pages run entirely in your browser. None of them calls a model or a server.

<ul class="demo-links">
  <li><a href="demos/chat/"><strong>Chat replay</strong><span>The chat example with the real wasm core and one canned turn: streamed text, a bills card, a tappable chart and the confirm flow.</span></a></li>
  <li><a href="demos/dashboard/"><strong>Dashboard</strong><span>The <code>ui.open</code> a live run of the shop host produced, drawn by the real wasm core and the web renderer.</span></a></li>
  <li><a href="demos/renderer/"><strong>Renderer</strong><span>The web renderer driven by a scripted core in TypeScript, with every message on the wire listed beside it.</span></a></li>
</ul>

## Chat replay

The page is `examples/chat` built with `--replay`. Instead of a WebSocket it uses a small class in the browser that answers with the messages from `examples/chat/dev/conversation.json`, the same ones the local dev server sends. The rest is the code that ships: the wasm client core applies the messages, checks URLs and fills defaults, and the renderer draws the ops it returns. The chart scene in that file was compiled by `polyui-chart`.

Try this: send any message, tap the July bar, then press Pay and confirm. The microphone and read aloud buttons work where the browser supports speech. Spoken words are first matched against the card by the core's voice matcher, and anything that does not match is sent on as a message.

<img src="docs/images/chat-answer.gif" width="640" alt="The canned answer arrives, with the chart and the stats">

## Dashboard

`crates/polyui-server/tests/fixtures/shop_dashboard_open.json` is the message a live run against `claude-haiku-5-5` produced for "How is the store doing this year? Build me a dashboard." with the shop host. The numbers came from read-only SQL over the demo data, not from the model. This page feeds that message to the wasm core in your browser. The [dashboards guide](docs/dashboards.md) explains how it was built.

![The shop dashboard: two stats above a line chart and a bar chart](docs/images/dashboard.png)

## Renderer

`js/polyui-web/demo` is the renderer's own demo page. It uses a scripted stand-in for the core written in TypeScript (`test/fake-core.ts`) rather than the wasm build, so it shows the renderer on its own: create, update and remove ops, the confirm sheet, choices, and reading aloud with highlights.

## Embedded MCP App

The embed demo needs a sandbox page on a second origin, which a single GitHub Pages site does not have, so it is not live here. This recording comes from the browser check in `js/polyui-web/scripts/embed-check.mjs`, which serves the host page and the sandbox on two local origins:

<img src="docs/images/embed.gif" width="600" alt="An MCP App inside a PolyUI card resizes itself, then its button sends a message that becomes a ui.action">
