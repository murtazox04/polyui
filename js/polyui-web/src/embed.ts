import type { ConfirmHandle } from "./confirm";
import { choice, element, isRecord, setShown, setText, text } from "./dom";
import { safeUrl } from "./url";
import type { ComponentRenderer, Json, Props } from "./types";

export interface AppAnswer {
  readonly result?: Json | undefined;
  readonly error?: { readonly code: number; readonly message: string };
}

export interface EmbedContext {
  readonly sandbox: string | null;
  confirmLink(url: string, done: (accepted: boolean) => void): ConfirmHandle;
  openLink(url: string): void;
  appCalls(): boolean;
  callApp(surface: string, embed: string, method: string, params: Json | undefined, done: (answer: AppAnswer) => void): () => void;
}

type JsonObject = { [key: string]: Json };

interface App {
  readonly html: string;
  readonly csp: { [key: string]: string[] };
  readonly input: JsonObject;
  readonly result: JsonObject | null;
}

export const mcpAppMimeType = "text/html;profile=mcp-app";
const sizes = ["small", "medium", "large"] as const;
const bounds: { readonly [key in (typeof sizes)[number]]: readonly [number, number] } = { small: [160, 320], medium: [320, 640], large: [480, 960] };
const minHeight = 80;
const messageBytes = 4096;
const messageGapMs = 1000;
const openAppCalls = 4;
const proxiedMethods = ["tools/call", "resources/read"];
const cspKeys = ["connectDomains", "resourceDomains", "frameDomains", "baseUriDomains"];
const sourcePattern = /^(https|wss):\/\/(\*\.)?[a-z0-9-]+(\.[a-z0-9-]+)*(:\d{1,5})?$/i;

export function embedSandbox(raw: unknown, allowHttp: boolean, hostOrigin: string): string | null {
  const href = safeUrl(raw, allowHttp);
  return href && new URL(href).origin !== hostOrigin ? href : null;
}

export function cleanCsp(value: Json | undefined): { [key: string]: string[] } {
  const csp: { [key: string]: string[] } = {};
  if (!isRecord(value)) return csp;
  for (const key of cspKeys) {
    const list = value[key];
    if (!Array.isArray(list)) continue;
    const kept = list.filter((item): item is string => typeof item === "string" && sourcePattern.test(item)).slice(0, 32);
    if (kept.length > 0) csp[key] = kept;
  }
  return csp;
}

function readApp(props: Props): App | null {
  const app = props["app"];
  if (!isRecord(app)) return null;
  const resource = app["resource"];
  if (!isRecord(resource) || resource["mimeType"] !== mcpAppMimeType || resource["uri"] !== props["resource"]) return null;
  const html = resource["text"];
  const meta = resource["_meta"];
  const ui = isRecord(meta) ? meta["ui"] : undefined;
  const input = app["tool_input"];
  const result = app["tool_result"];
  if (typeof html !== "string") return null;
  return { html, csp: cleanCsp(isRecord(ui) ? ui["csp"] : undefined), input: isRecord(input) ? input : {}, result: isRecord(result) ? result : null };
}

type Theme = "light" | "dark";

const darkQuery = "(prefers-color-scheme: dark)";

function systemDark(): MediaQueryList | null {
  return typeof window.matchMedia === "function" ? window.matchMedia(darkQuery) : null;
}

export function themeOf(node: Element): Theme {
  const schemes = getComputedStyle(node).getPropertyValue("color-scheme").split(/\s+/).filter((word) => word === "light" || word === "dark");
  if (schemes.length === 1) return schemes[0] as Theme;
  return systemDark()?.matches ? "dark" : "light";
}

function messageText(params: Json | undefined): string {
  if (!isRecord(params) || params["role"] !== "user") return "";
  const content = params["content"];
  const blocks = Array.isArray(content) ? content : [content];
  return blocks
    .map((block) => (isRecord(block) && block["type"] === "text" && typeof block["text"] === "string" ? block["text"] : ""))
    .filter((part) => part !== "")
    .join("\n");
}

export function createEmbedRenderer(context: EmbedContext): ComponentRenderer {
  return (api) => {
    const node = element("figure", "pui-embed");
    const alt = element("p", "pui-embed-alt", node);
    let frame: HTMLIFrameElement | null = null;
    let port: MessagePort | null = null;
    let app: App | null = null;
    let props: Props = {};
    let maxHeight = bounds.medium[1];
    let loadedHtml: string | null = null;
    let ready = false;
    let inputSent = false;
    let sentResult = "";
    let lastMessage = -messageGapMs;
    let link: ConfirmHandle | null = null;
    let theme: Theme | null = null;
    const system = systemDark();
    const calls = new Set<() => void>();

    const post = (message: JsonObject): void => port?.postMessage({ jsonrpc: "2.0", ...message });
    const reply = (id: Json, result: JsonObject): void => post({ id, result });
    const fail = (id: Json, code: number, message: string): void => post({ id, error: { code, message } });

    const sendTool = (): void => {
      if (!ready || !app) return;
      if (!inputSent) post({ method: "ui/notifications/tool-input", params: { arguments: app.input } });
      inputSent = true;
      const result = app.result ? JSON.stringify(app.result) : sentResult;
      if (app.result && result !== sentResult) post({ method: "ui/notifications/tool-result", params: app.result });
      sentResult = result;
    };

    const request = (id: Json, method: string, params: Json | undefined): void => {
      if (method === "ui/initialize") {
        reply(id, {
          protocolVersion: "2026-01-26",
          hostInfo: { name: "polyui-web", version: "0.1.0" },
          hostCapabilities: context.appCalls() ? { openLinks: {}, serverTools: {}, serverResources: {} } : { openLinks: {} },
          hostContext: {
            theme: (theme = themeOf(node)),
            displayMode: "inline",
            availableDisplayModes: ["inline"],
            containerDimensions: { maxHeight },
            platform: "web",
            locale: document.documentElement.lang || navigator.language,
          },
        });
      } else if (method === "ping") {
        reply(id, {});
      } else if (method === "ui/request-display-mode") {
        reply(id, { mode: "inline" });
      } else if (method === "ui/message") {
        const said = messageText(params);
        const now = Date.now();
        if (!isRecord(props["on_message"])) return fail(id, -32000, "Message sending denied");
        if (said === "" || new TextEncoder().encode(JSON.stringify({ text: said })).length > messageBytes) return fail(id, -32602, "Invalid message format");
        if (now - lastMessage < messageGapMs) return fail(id, -32000, "Too many messages");
        lastMessage = now;
        api.fire("on_message", { text: said });
        reply(id, {});
      } else if (method === "ui/open-link") {
        const href = safeUrl(isRecord(params) ? params["url"] : undefined);
        if (!href) return fail(id, -32000, "Invalid URL");
        if (link) return fail(id, -32000, "Policy violation");
        link = context.confirmLink(href, (accepted) => {
          link = null;
          if (!accepted) return fail(id, -32000, "Link opening denied by user");
          context.openLink(href);
          reply(id, {});
        });
      } else if (proxiedMethods.includes(method) && context.appCalls()) {
        if (calls.size >= openAppCalls) return fail(id, -32000, "Too many requests are still open");
        const cancel = context.callApp(api.surface, api.id, method, params, (answer) => {
          calls.delete(cancel);
          if (answer.error) return fail(id, answer.error.code, answer.error.message);
          if (!isRecord(answer.result)) return fail(id, -32603, "Invalid result");
          reply(id, answer.result);
        });
        calls.add(cancel);
      } else {
        fail(id, -32601, "Method not found");
      }
    };

    const sendTheme = (): void => {
      if (!ready) return;
      const now = themeOf(node);
      if (now === theme) return;
      theme = now;
      post({ method: "ui/notifications/host-context-changed", params: { theme } });
    };

    const fromView = (event: MessageEvent): void => {
      const data = event.data as Json;
      if (!isRecord(data) || data["jsonrpc"] !== "2.0" || typeof data["method"] !== "string") return;
      const method = data["method"];
      const id = data["id"];
      if (typeof id === "string" || typeof id === "number") return request(id, method, data["params"]);
      if (method === "ui/notifications/initialized") {
        ready = true;
        sendTool();
      } else if (method === "ui/notifications/size-changed" && frame) {
        const params = data["params"];
        const height = isRecord(params) ? params["height"] : undefined;
        if (typeof height === "number" && Number.isFinite(height)) frame.style.height = `${Math.min(maxHeight, Math.max(minHeight, Math.ceil(height)))}px`;
      }
    };

    const fromSandbox = (event: MessageEvent): void => {
      const target = frame?.contentWindow;
      if (!target || event.source !== target || event.origin !== "null" || port || !app) return;
      const data = event.data as Json;
      if (!isRecord(data) || data["method"] !== "ui/notifications/sandbox-proxy-ready") return;
      const channel = new MessageChannel();
      port = channel.port1;
      port.addEventListener("message", fromView);
      port.start();
      target.postMessage({ jsonrpc: "2.0", method: "ui/notifications/sandbox-resource-ready", params: { html: app.html, csp: app.csp } }, "*", [channel.port2]);
    };

    const teardown = (): void => {
      window.removeEventListener("message", fromSandbox);
      system?.removeEventListener("change", sendTheme);
      for (const cancel of calls) cancel();
      calls.clear();
      if (ready) post({ id: 0, method: "ui/resource-teardown", params: { reason: "removed" } });
      port?.close();
      frame?.remove();
      link?.close();
      frame = port = link = loadedHtml = null;
      ready = inputSent = false;
      sentResult = "";
      theme = null;
    };

    return {
      element: node,
      update(state) {
        props = state.props;
        app = context.sandbox ? readApp(props) : null;
        const title = text(props, "title");
        setText(alt, state.a11y?.label || title);
        setShown(alt, !app);
        const [initial, max] = bounds[choice(props, "height", sizes, "medium")];
        maxHeight = max;
        if (!app || !context.sandbox) return teardown();
        if (frame) frame.title = title;
        if (app.html === loadedHtml) {
          sendTheme();
          return sendTool();
        }
        teardown();
        loadedHtml = app.html;
        frame = element("iframe", "pui-embed-frame", node);
        frame.setAttribute("sandbox", "allow-scripts");
        frame.setAttribute("referrerpolicy", "origin");
        frame.title = title;
        frame.style.height = `${initial}px`;
        window.addEventListener("message", fromSandbox);
        system?.addEventListener("change", sendTheme);
        frame.src = context.sandbox;
      },
      destroy: teardown,
    };
  };
}
