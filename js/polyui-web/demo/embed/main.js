import { loadCore, mount } from "../../dist/polyui.js";

const sandbox = "http://127.0.0.1:4191/";
const uri = "ui://weather/forecast";
const hello = { v: 1, t: "hello", client: { name: "polyui-web", version: "0.1.0" }, catalogs: ["core@1", "embed@1"], modalities: ["screen"], locale: "en" };

const wire = document.querySelector("#wire");
const core = await loadCore(hello);
const renderer = mount(document.querySelector("#stage"), {
  core,
  allowHttp: true,
  embedOrigin: sandbox,
  send: (message) => {
    const item = document.createElement("li");
    item.textContent = message;
    wire.append(item);
  },
});

const html = await (await fetch("./forecast-app.html")).text();
const days = [
  { name: "Tue", high: 14 },
  { name: "Wed", high: 16 },
  { name: "Thu", high: 12 },
  { name: "Fri", high: 9 },
  { name: "Sat", high: 11 },
];

renderer.receive({ v: 1, t: "welcome", session: "demo", catalogs: ["core@1", "embed@1"] });
renderer.receive({
  v: 1,
  t: "ui.open",
  surface: "s1",
  seq: 1,
  root: "card",
  data: {},
  components: [
    { id: "card", type: "Card", props: { title: "Weather this week" }, children: ["intro", "forecast"] },
    { id: "intro", type: "Text", props: { text: "The forecast tool has its own view, so the assistant shows it in a sandbox.", style: "muted" } },
    {
      id: "forecast",
      type: "embed:Embed",
      props: {
        resource: uri,
        title: "Five day forecast for Tashkent",
        height: "small",
        on_message: { action_id: "act_ask", confirm: false },
        app: {
          resource: { uri, mimeType: "text/html;profile=mcp-app", text: html, _meta: { ui: { csp: {} } } },
          tool_input: { city: "Tashkent" },
          tool_result: { content: [{ type: "text", text: "Mild days, a cold front on Friday." }], structuredContent: { days } },
        },
      },
    },
  ],
});
