import { loadCore, mount } from "../../../js/polyui-web/src/index";
import dashboard from "../../../crates/polyui-server/tests/fixtures/shop_dashboard_open.json";

const hello = {
  v: 1,
  t: "hello",
  client: { name: "polyui-site-dashboard", version: "0.1.0" },
  catalogs: ["core@1", "chart@1"],
  modalities: ["screen"],
  locale: "en",
};

function required<T extends Element>(selector: string): T {
  const found = document.querySelector<T>(selector);
  if (!found) throw new Error(`the dashboard page is missing ${selector}`);
  return found;
}

const stage = required<HTMLElement>("#stage");
const status = required<HTMLParagraphElement>("#status");

async function start(): Promise<void> {
  const core = await loadCore(hello, new URL("./polyui_core_bg.wasm", import.meta.url));
  const renderer = mount(stage, { core, send: () => undefined });
  renderer.receive({ v: 1, t: "welcome", session: "site", catalogs: ["core@1", "chart@1"] });
  renderer.receive(JSON.stringify(dashboard));
  status.dataset["state"] = "live";
  status.textContent = "Drawn by the wasm core";
}

start().catch((error: unknown) => {
  status.dataset["state"] = "offline";
  status.textContent = error instanceof Error ? error.message : String(error);
});
