import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanCsp, embedSandbox, mcpAppMimeType } from "../src/embed";
import type { Json, Props, RendererOptions } from "../src/types";
import { harness, type Harness } from "./setup";

const hostUrl = "http://localhost:4190/chat";
const sandboxUrl = "http://127.0.0.1:4191/embed-sandbox/";
const uri = "ui://weather/forecast";
type JsonObject = { [key: string]: Json };

function app(overrides: JsonObject = {}): JsonObject {
  return {
    resource: {
      uri,
      mimeType: mcpAppMimeType,
      text: "<!doctype html><p>Forecast</p>",
      "_meta": { ui: { csp: { connectDomains: ["https://api.example.com", "'unsafe-eval'", "https://a.com; script-src *"] } } },
    },
    tool_input: { city: "Tashkent" },
    ...overrides,
  };
}

function play(h: Harness, props: Props, height = "medium"): void {
  h.play({
    surface: "s1",
    ops: [
      { op: "create", id: "app", type: "embed:Embed", props: { resource: uri, title: "Forecast", height, ...props }, a11y: { role: "figure", label: "Forecast" } },
      { op: "root", id: "app" },
    ],
  });
}

function setup(options: Partial<RendererOptions> = {}): Harness {
  return harness({ embedOrigin: sandboxUrl, allowHttp: true, ...options });
}

interface Connection {
  readonly frame: HTMLIFrameElement;
  readonly posted: { message: JsonObject; target: string }[];
  readonly replies: JsonObject[];
  send(message: JsonObject): Promise<void>;
}

const tick = (): Promise<void> => new Promise((done) => setTimeout(done, 5));

interface FakeFrameWindow {
  readonly window: Window;
  readonly calls: [JsonObject, string, readonly Transferable[]][];
}

function fakeFrameWindow(frame: HTMLIFrameElement): FakeFrameWindow {
  const calls: [JsonObject, string, readonly Transferable[]][] = [];
  const fake = { postMessage: (message: JsonObject, targetOrigin: string, transfer: Transferable[] = []) => calls.push([message, targetOrigin, transfer]) };
  Object.defineProperty(frame, "contentWindow", { configurable: true, get: () => fake });
  return { window: fake as unknown as Window, calls };
}

function sandboxReady(source: Window, origin = "null"): void {
  window.dispatchEvent(new MessageEvent("message", { data: { jsonrpc: "2.0", method: "ui/notifications/sandbox-proxy-ready", params: {} }, origin, source }));
}

async function connect(h: Harness, props: Props = { app: app(), on_message: { action_id: "act_ask" } }, height = "medium"): Promise<Connection> {
  play(h, props, height);
  const frame = h.find<HTMLIFrameElement>("iframe.pui-embed-frame");
  const target = fakeFrameWindow(frame);
  sandboxReady(target.window);
  const posted = target.calls.map(([message, targetOrigin]) => ({ message, target: targetOrigin }));
  const ports = target.calls.flatMap(([, , transfer]) => transfer.filter((item): item is MessagePort => typeof (item as MessagePort).postMessage === "function"));
  const port = ports[0];
  if (!port) throw new Error("no port handed over");
  const replies: JsonObject[] = [];
  port.onmessage = (event: MessageEvent) => replies.push(event.data as JsonObject);
  return {
    frame,
    posted,
    replies,
    send: async (message) => {
      port.postMessage({ jsonrpc: "2.0", ...message });
      await tick();
    },
  };
}

beforeEach(() => {
  (window as unknown as { happyDOM?: { setURL(url: string): void } }).happyDOM?.setURL(hostUrl);
});

afterEach(() => {
  vi.restoreAllMocks();
});

describe("embed:Embed refusal", () => {
  it("shows the title and no frame when the host set no sandbox origin", () => {
    const h = harness();
    play(h, { app: app() });
    expect(document.querySelector("iframe")).toBeNull();
    expect(h.find(".pui-embed-alt").textContent).toBe("Forecast");
  });

  it("refuses a sandbox on the host's own origin or with a bad URL", () => {
    expect(embedSandbox("http://localhost:4190/sandbox/", true, "http://localhost:4190")).toBeNull();
    expect(embedSandbox("javascript:alert(1)", true, "http://localhost:4190")).toBeNull();
    expect(embedSandbox(sandboxUrl, false, "http://localhost:4190")).toBeNull();
    expect(embedSandbox(sandboxUrl, true, "http://localhost:4190")).toBe(sandboxUrl);
    const h = setup({ embedOrigin: "http://localhost:4190/sandbox/" });
    play(h, { app: app() });
    expect(document.querySelector("iframe")).toBeNull();
    expect(h.find(".pui-embed-alt").hidden).toBe(false);
  });

  it("refuses apps without the MCP Apps mime type or with another URI", () => {
    for (const bad of [app({ resource: { uri, mimeType: "text/html", text: "<p>x</p>" } }), app({ resource: { uri: "ui://other", mimeType: mcpAppMimeType, text: "<p>x</p>" } }), "<p>x</p>"]) {
      const h = setup();
      play(h, { app: bad });
      expect(document.querySelector("iframe")).toBeNull();
      expect(h.find(".pui-embed-alt").textContent).toBe("Forecast");
    }
  });
});

describe("embed:Embed frame", () => {
  it("loads the sandbox page with allow-scripts only and never puts the HTML in the host page", () => {
    const h = setup();
    play(h, { app: app() }, "small");
    const frame = h.find<HTMLIFrameElement>("iframe.pui-embed-frame");
    expect(frame.getAttribute("sandbox")).toBe("allow-scripts");
    expect(frame.getAttribute("sandbox")).not.toContain("allow-same-origin");
    expect(frame.hasAttribute("srcdoc")).toBe(false);
    expect(frame.getAttribute("src")).toBe(sandboxUrl);
    expect(frame.getAttribute("referrerpolicy")).toBe("origin");
    expect(frame.title).toBe("Forecast");
    expect(frame.style.height).toBe("160px");
    expect(h.find(".pui-embed-alt").hidden).toBe(true);
    expect(document.documentElement.outerHTML).not.toContain("<p>Forecast</p>");
  });

  it("answers the sandbox only for its own frame with an opaque origin", async () => {
    const h = setup();
    play(h, { app: app() });
    const frame = h.find<HTMLIFrameElement>("iframe.pui-embed-frame");
    const target = fakeFrameWindow(frame);
    sandboxReady(target.window, "http://evil.test");
    sandboxReady(window);
    sandboxReady(target.window, "http://localhost:4190");
    expect(target.calls).toHaveLength(0);
    sandboxReady(target.window);
    sandboxReady(target.window);
    expect(target.calls).toHaveLength(1);
    const [message, targetOrigin, transfer] = target.calls[0] ?? [{} as JsonObject, "", []];
    expect(message["method"]).toBe("ui/notifications/sandbox-resource-ready");
    expect(message["params"]).toEqual({ html: "<!doctype html><p>Forecast</p>", csp: { connectDomains: ["https://api.example.com"] } });
    expect(targetOrigin).toBe("*");
    expect(typeof (transfer[0] as MessagePort | undefined)?.postMessage).toBe("function");
  });

  it("keeps only plain origins in the CSP it forwards", () => {
    expect(cleanCsp({ resourceDomains: ["https://cdn.example.com", "https://*.cdn.example.com:8443", "http://cdn.example.com", "*", "'self'", "data:", "https://a.com/path", 7], frameDomains: "https://x.com" })).toEqual({
      resourceDomains: ["https://cdn.example.com", "https://*.cdn.example.com:8443"],
    });
    expect(cleanCsp(null)).toEqual({});
  });
});

describe("embed:Embed messages", () => {
  it("initializes, then sends tool input and each new tool result once", async () => {
    const h = setup();
    const view = await connect(h);
    await view.send({ id: 1, method: "ui/initialize", params: { appInfo: { name: "Weather", version: "1" }, appCapabilities: {}, protocolVersion: "2026-01-26" } });
    const init = view.replies[0];
    expect(init?.["id"]).toBe(1);
    expect((init?.["result"] as JsonObject)["protocolVersion"]).toBe("2026-01-26");
    expect(((init?.["result"] as JsonObject)["hostContext"] as JsonObject)["containerDimensions"]).toEqual({ maxHeight: 640 });
    expect(view.replies).toHaveLength(1);
    await view.send({ method: "ui/notifications/initialized" });
    expect(view.replies[1]).toEqual({ jsonrpc: "2.0", method: "ui/notifications/tool-input", params: { arguments: { city: "Tashkent" } } });
    const result = { content: [{ type: "text", text: "Sunny" }], structuredContent: { temperature: 21 } };
    play(h, { app: app({ tool_result: result }), on_message: { action_id: "act_ask" } });
    play(h, { app: app({ tool_result: result }), on_message: { action_id: "act_ask" } });
    await tick();
    expect(view.replies.slice(2)).toEqual([{ jsonrpc: "2.0", method: "ui/notifications/tool-result", params: result }]);
    await view.send({ id: 2, method: "tools/call", params: { name: "x", arguments: {} } });
    expect(view.replies.at(-1)).toEqual({ jsonrpc: "2.0", id: 2, error: { code: -32601, message: "Method not found" } });
  });

  it("tells the app the host theme and sends a change when the system theme flips", async () => {
    const listeners: (() => void)[] = [];
    const query = { matches: true, addEventListener: (_: string, listener: () => void) => listeners.push(listener), removeEventListener: vi.fn() };
    vi.stubGlobal("matchMedia", vi.fn(() => query));
    try {
      const h = setup();
      const view = await connect(h);
      await view.send({ id: 1, method: "ui/initialize", params: { appInfo: { name: "Weather", version: "1" }, appCapabilities: {}, protocolVersion: "2026-01-26" } });
      expect(((view.replies[0]?.["result"] as JsonObject)["hostContext"] as JsonObject)["theme"]).toBe("dark");
      await view.send({ method: "ui/notifications/initialized" });
      const before = view.replies.length;
      query.matches = false;
      for (const listener of listeners) listener();
      await tick();
      expect(view.replies.slice(before)).toEqual([{ jsonrpc: "2.0", method: "ui/notifications/host-context-changed", params: { theme: "light" } }]);
      for (const listener of listeners) listener();
      await tick();
      expect(view.replies).toHaveLength(before + 1);
    } finally {
      vi.unstubAllGlobals();
    }
  });

  it("uses the surface's own color scheme over the system one", async () => {
    vi.stubGlobal("matchMedia", vi.fn(() => ({ matches: true, addEventListener: vi.fn(), removeEventListener: vi.fn() })));
    try {
      const h = setup();
      const view = await connect(h);
      view.frame.parentElement?.style.setProperty("color-scheme", "light");
      await view.send({ id: 1, method: "ui/initialize", params: { appInfo: { name: "Weather", version: "1" }, appCapabilities: {}, protocolVersion: "2026-01-26" } });
      expect(((view.replies[0]?.["result"] as JsonObject)["hostContext"] as JsonObject)["theme"]).toBe("light");
    } finally {
      vi.unstubAllGlobals();
    }
  });

  it("resizes the frame within the height hint's bounds", async () => {
    const h = setup();
    const view = await connect(h, { app: app() }, "large");
    await view.send({ method: "ui/notifications/size-changed", params: { width: 300, height: 5000 } });
    expect(view.frame.style.height).toBe("960px");
    await view.send({ method: "ui/notifications/size-changed", params: { height: 3 } });
    expect(view.frame.style.height).toBe("80px");
    await view.send({ method: "ui/notifications/size-changed", params: { height: 412.2 } });
    expect(view.frame.style.height).toBe("413px");
    await view.send({ method: "ui/notifications/size-changed", params: { height: "9999px" } });
    expect(view.frame.style.height).toBe("413px");
  });

  it("turns ui/message into the component's on_message action with the text as detail", async () => {
    const h = setup();
    const view = await connect(h);
    await view.send({ id: 7, method: "ui/message", params: { role: "user", content: [{ type: "text", text: "Show Samarkand too" }] } });
    expect(h.core.calls.filter((call) => call.method === "fire").map((call) => call.args)).toEqual([["s1", "app", "on_message", JSON.stringify({ text: "Show Samarkand too" })]]);
    expect(h.messages().at(-1)).toMatchObject({ t: "ui.action", action_id: "act_ask", detail: { text: "Show Samarkand too" } });
    expect(view.replies.at(-1)).toEqual({ jsonrpc: "2.0", id: 7, result: {} });
    await view.send({ id: 8, method: "ui/message", params: { role: "user", content: { type: "text", text: "again" } } });
    expect(view.replies.at(-1)).toEqual({ jsonrpc: "2.0", id: 8, error: { code: -32000, message: "Too many messages" } });
    await view.send({ id: 9, method: "ui/message", params: { role: "user", content: [{ type: "text", text: "x".repeat(5000) }] } });
    expect(view.replies.at(-1)?.["error"]).toEqual({ code: -32602, message: "Invalid message format" });
    expect(h.core.calls.filter((call) => call.method === "fire")).toHaveLength(1);
  });

  it("denies ui/message when the component has no on_message action", async () => {
    const h = setup();
    const view = await connect(h, { app: app() });
    await view.send({ id: 3, method: "ui/message", params: { role: "user", content: [{ type: "text", text: "hi" }] } });
    expect(view.replies.at(-1)).toEqual({ jsonrpc: "2.0", id: 3, error: { code: -32000, message: "Message sending denied" } });
    expect(h.core.calls.some((call) => call.method === "fire")).toBe(false);
  });

  it("opens only https links, and only after the user confirms in the renderer's sheet", async () => {
    const opened: string[] = [];
    const h = setup({ openLink: (url) => opened.push(url) });
    const view = await connect(h);
    for (const url of ["http://example.com", "javascript:alert(1)", "https://user:pw@example.com", 42]) {
      await view.send({ id: 1, method: "ui/open-link", params: { url } });
      expect(view.replies.at(-1)?.["error"]).toEqual({ code: -32000, message: "Invalid URL" });
    }
    expect(document.querySelector("dialog.pui-confirm")).toBeNull();
    await view.send({ id: 2, method: "ui/open-link", params: { url: "https://example.com/forecast" } });
    const dialog = h.find<HTMLDialogElement>("dialog.pui-confirm");
    expect(dialog.querySelector(".pui-confirm-body")?.textContent).toBe("https://example.com/forecast");
    await view.send({ id: 3, method: "ui/open-link", params: { url: "https://example.com/other" } });
    expect(view.replies.at(-1)?.["error"]).toEqual({ code: -32000, message: "Policy violation" });
    const [cancel, accept] = [...dialog.querySelectorAll<HTMLButtonElement>("button")];
    accept?.click();
    await tick();
    expect(opened).toEqual(["https://example.com/forecast"]);
    expect(view.replies.at(-1)).toEqual({ jsonrpc: "2.0", id: 2, result: {} });
    await view.send({ id: 4, method: "ui/open-link", params: { url: "https://example.com/again" } });
    h.find<HTMLDialogElement>("dialog.pui-confirm").querySelector<HTMLButtonElement>("button")?.click();
    await tick();
    expect(cancel).toBeDefined();
    expect(opened).toHaveLength(1);
    expect(view.replies.at(-1)).toEqual({ jsonrpc: "2.0", id: 4, error: { code: -32000, message: "Link opening denied by user" } });
  });

  it("removes the frame and its listener when the component goes away", async () => {
    const h = setup();
    const view = await connect(h);
    h.play({ surface: "s1", ops: [{ op: "remove", id: "app" }] });
    expect(view.frame.isConnected).toBe(false);
  });
});

describe("embed:Embed app calls", () => {
  const initialize = { id: 1, method: "ui/initialize", params: { appInfo: { name: "Weather", version: "1" }, appCapabilities: {}, protocolVersion: "2026-01-26" } };
  const welcome = (features: string[]): string => JSON.stringify({ v: 1, t: "welcome", session: "sess_1", catalogs: ["core@1", "embed@1"], features });
  const appCalls = (h: Harness): { [key: string]: unknown }[] => h.messages().filter((message) => message["t"] === "app.call");
  const capabilities = (view: Connection): JsonObject => (view.replies[0]?.["result"] as JsonObject)["hostCapabilities"] as JsonObject;

  it("refuses server requests and advertises no server tools until the server accepts app calls", async () => {
    const h = setup();
    h.renderer.receive(welcome(["resume"]));
    const view = await connect(h);
    await view.send(initialize);
    expect(capabilities(view)).toEqual({ openLinks: {} });
    await view.send({ id: 2, method: "tools/call", params: { name: "refresh", arguments: {} } });
    expect(view.replies.at(-1)).toEqual({ jsonrpc: "2.0", id: 2, error: { code: -32601, message: "Method not found" } });
    expect(appCalls(h)).toEqual([]);
  });

  it("carries tools/call and resources/read to the server and the answers back to the app", async () => {
    const h = setup();
    h.renderer.receive(welcome(["stream", "app_calls"]));
    const view = await connect(h);
    await view.send(initialize);
    expect(capabilities(view)).toEqual({ openLinks: {}, serverTools: {}, serverResources: {} });

    await view.send({ id: 7, method: "tools/call", params: { name: "forecast", arguments: { city: "Samarkand" } } });
    await view.send({ id: "read-1", method: "resources/read", params: { uri } });
    const [call, read] = appCalls(h);
    expect(call).toEqual({ v: 1, t: "app.call", surface: "s1", embed: "app", id: "a1", method: "tools/call", params: { name: "forecast", arguments: { city: "Samarkand" } } });
    expect(read).toMatchObject({ embed: "app", id: "a2", method: "resources/read", params: { uri } });

    const result = { content: [{ type: "text", text: "Rain" }], structuredContent: { city: "Samarkand" } };
    h.renderer.receive(JSON.stringify({ v: 1, t: "app.result", surface: "s1", embed: "app", id: "a1", result }));
    await tick();
    expect(view.replies.at(-1)).toEqual({ jsonrpc: "2.0", id: 7, result });

    h.renderer.receive(JSON.stringify({ v: 1, t: "app.result", surface: "s1", embed: "app", id: "a2", error: { code: -32000, message: "unknown resource" } }));
    await tick();
    expect(view.replies.at(-1)).toEqual({ jsonrpc: "2.0", id: "read-1", error: { code: -32000, message: "unknown resource" } });

    const before = view.replies.length;
    h.renderer.receive(JSON.stringify({ v: 1, t: "app.result", surface: "s1", embed: "app", id: "a1", result }));
    h.renderer.receive(JSON.stringify({ v: 1, t: "app.result", surface: "s1", embed: "other", id: "a3", result }));
    await tick();
    expect(view.replies).toHaveLength(before);
    expect(h.core.calls.filter((call) => call.method === "receive" && String(call.args[0]).includes("app.result"))).toEqual([]);
  });

  it("refuses an answer that is not an object and caps the open requests", async () => {
    const h = setup();
    h.renderer.receive(welcome(["app_calls"]));
    const view = await connect(h);
    await view.send(initialize);
    for (const id of [1, 2, 3, 4, 5]) await view.send({ id: 10 + id, method: "tools/call", params: { name: "slow" } });
    expect(appCalls(h)).toHaveLength(4);
    expect(view.replies.at(-1)).toEqual({ jsonrpc: "2.0", id: 15, error: { code: -32000, message: "Too many requests are still open" } });
    h.renderer.receive(JSON.stringify({ v: 1, t: "app.result", surface: "s1", embed: "app", id: "a1", result: "done" }));
    await tick();
    expect(view.replies.at(-1)).toEqual({ jsonrpc: "2.0", id: 11, error: { code: -32603, message: "Invalid result" } });
    await view.send({ id: 16, method: "tools/call", params: { name: "slow" } });
    expect(appCalls(h)).toHaveLength(5);
  });

  it("drops answers for an embed that was removed", async () => {
    const h = setup();
    h.renderer.receive(welcome(["app_calls"]));
    const view = await connect(h);
    await view.send(initialize);
    await view.send({ method: "ui/notifications/initialized" });
    await view.send({ id: 3, method: "tools/call", params: { name: "forecast" } });
    h.play({ surface: "s1", ops: [{ op: "remove", id: "app" }] });
    const before = view.replies.length;
    h.renderer.receive(JSON.stringify({ v: 1, t: "app.result", surface: "s1", embed: "app", id: "a1", result: { content: [] } }));
    await tick();
    expect(view.replies.slice(before).filter((reply) => reply["id"] === 3)).toEqual([]);
  });

  it("times out a request the server never answers", async () => {
    const h = setup();
    h.renderer.receive(welcome(["app_calls"]));
    const view = await connect(h);
    await view.send(initialize);
    vi.useFakeTimers({ shouldAdvanceTime: true });
    try {
      await view.send({ id: 9, method: "tools/call", params: { name: "forecast" } });
      expect(view.replies.at(-1)?.["id"]).not.toBe(9);
      vi.advanceTimersByTime(90_000);
      await tick();
    } finally {
      vi.useRealTimers();
    }
    expect(view.replies.at(-1)).toEqual({ jsonrpc: "2.0", id: 9, error: { code: -32000, message: "The app request timed out." } });
  });
});
