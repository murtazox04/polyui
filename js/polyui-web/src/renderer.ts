import { createChartRenderer, renderDashboard } from "./chart";
import { coreRenderers } from "./components";
import { openConfirm, type ConfirmHandle } from "./confirm";
import { isRecord } from "./dom";
import { createEmbedRenderer, embedSandbox } from "./embed";
import { PolyuiSurface, defineSurfaceElement, surfaceTag, type SurfaceHost } from "./surface";
import { safeUrl } from "./url";
import type {
  ComponentApi,
  ComponentRenderer,
  ConfirmOp,
  Json,
  Labels,
  PolyuiCore,
  RenderBatch,
  RendererOptions,
  Utterance,
  VoiceMatch,
} from "./types";

const defaultLabels: Labels = {
  cancel: "Cancel",
  expired: "This request expired. Ask again to continue.",
  chartUnavailable: "Chart unavailable",
  openLinkTitle: "Open this link?",
  openLink: "Open",
};

function parse(raw: string | null): Json | undefined {
  if (raw === null) return undefined;
  try {
    return JSON.parse(raw) as Json;
  } catch {
    return undefined;
  }
}

function asBatch(value: Json | undefined): RenderBatch | null {
  if (!isRecord(value) || typeof value["surface"] !== "string") return null;
  const ops = value["ops"];
  const send = value["send"];
  if (ops !== undefined && !Array.isArray(ops)) return null;
  if (send !== undefined && !Array.isArray(send)) return null;
  return { ...value, ops: ops ?? [] } as unknown as RenderBatch;
}

function errorDetail(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

export class Renderer implements SurfaceHost {
  #core: PolyuiCore;
  #send: (message: string) => void;
  #container: Element | null;
  #sync: boolean;
  #allowHttp: boolean;
  #confirmDelayMs: number;
  #labels: Labels;
  #registry = new Map<string, ComponentRenderer>(coreRenderers);
  #surfaces = new Map<string, PolyuiSurface>();
  #confirms = new Map<string, ConfirmHandle>();
  #reported = new Set<string>();
  #queue: RenderBatch[] = [];
  #frame = 0;
  #destroyed = false;

  constructor(options: RendererOptions) {
    defineSurfaceElement();
    this.#core = options.core;
    this.#send = options.send;
    this.#container = options.container ?? null;
    this.#sync = options.sync ?? false;
    this.#allowHttp = options.allowHttp ?? false;
    this.#confirmDelayMs = options.confirmDelayMs ?? 400;
    this.#labels = { ...defaultLabels, ...options.labels };
    this.#registry.set("chart:Chart", createChartRenderer(() => this.#labels.chartUnavailable));
    this.#registry.set("chart:Dashboard", renderDashboard);
    const openLink = options.openLink ?? ((url: string) => void window.open(url, "_blank", "noopener,noreferrer"));
    this.#registry.set(
      "embed:Embed",
      createEmbedRenderer({
        sandbox: embedSandbox(options.embedOrigin, this.#allowHttp, location.origin),
        confirmLink: (url, done) =>
          openConfirm(
            { op: "confirm", action_id: "", nonce: "", title: this.#labels.openLinkTitle, body: url, confirm_label: this.#labels.openLink },
            { labels: this.#labels, delayMs: this.#confirmDelayMs, onConfirm: () => done(true), onDismiss: () => done(false) },
          ),
        openLink,
      }),
    );
  }

  define(type: string, renderer: ComponentRenderer): this {
    this.#registry.set(type, renderer);
    return this;
  }

  receive(message: string | { readonly [key: string]: Json }): void {
    if (this.#destroyed) return;
    const text = typeof message === "string" ? message : JSON.stringify(message);
    this.#accept(this.#call("", () => this.#core.receive(text)));
  }

  surface(id: string): PolyuiSurface {
    let node = this.#surfaces.get(id);
    if (!node) {
      node = document.createElement(surfaceTag) as PolyuiSurface;
      node.setAttribute("surface", id);
      this.attach(node);
      this.#container?.append(node);
    }
    return node;
  }

  attach(node: PolyuiSurface): void {
    node.attachHost(this);
    this.#surfaces.set(node.surface, node);
  }

  highlight(surface: string, id: string | null): void {
    this.#enqueue({ surface, ops: [{ op: "highlight", id }] });
  }

  say(surface: string): Utterance[] {
    const value = parse(this.#call(surface, () => this.#core.say(surface)));
    if (!Array.isArray(value)) return [];
    return value.filter((item): item is { id: string; text: string } => isRecord(item) && typeof item["id"] === "string" && typeof item["text"] === "string");
  }

  matchVoice(surface: string, text: string): VoiceMatch | null {
    const value = parse(this.#call(surface, () => this.#core.matchVoice(surface, text)));
    if (!isRecord(value) || typeof value["component"] !== "string") return null;
    const batch = asBatch(value["batch"]);
    if (batch) this.#enqueue(batch);
    const action = value["action"];
    if (isRecord(action)) this.#transmit(action);
    return value as unknown as VoiceMatch;
  }

  flush(): void {
    if (this.#frame) cancelAnimationFrame(this.#frame);
    this.#frame = 0;
    if (this.#queue.length === 0) return;
    const batches = this.#queue.splice(0);
    const doc = this.#container?.ownerDocument ?? document;
    const active = doc.activeElement;
    const field = active instanceof HTMLInputElement || active instanceof HTMLTextAreaElement ? active : null;
    const selection = field ? [field.selectionStart, field.selectionEnd] : null;
    for (const batch of batches) this.surface(batch.surface).apply(batch);
    if (active instanceof HTMLElement || active instanceof SVGElement) {
      if (active.isConnected && doc.activeElement !== active) {
        active.focus({ preventScroll: true });
        const [start, end] = selection ?? [null, null];
        if (field && typeof start === "number" && typeof end === "number") field.setSelectionRange(start, end);
      }
    }
  }

  destroy(): void {
    this.#destroyed = true;
    if (this.#frame) cancelAnimationFrame(this.#frame);
    this.#frame = 0;
    this.#queue.length = 0;
    for (const handle of this.#confirms.values()) handle.close();
    this.#confirms.clear();
    for (const node of this.#surfaces.values()) {
      node.dispose();
      node.remove();
    }
    this.#surfaces.clear();
  }

  resolve(type: string): ComponentRenderer | undefined {
    return this.#registry.get(type);
  }

  api(surface: string, id: string, type: string): ComponentApi {
    return {
      surface,
      id,
      type,
      setValue: (path, value) => {
        if (this.#destroyed) return;
        this.#accept(this.#call(surface, () => this.#core.setValue(surface, path, JSON.stringify(value))));
      },
      fire: (prop, detail) => {
        if (this.#destroyed) return;
        const detailJson = detail === undefined ? undefined : JSON.stringify(detail);
        const message = this.#call(surface, () => this.#core.fire(surface, id, prop, detailJson));
        if (message) this.#send(message);
      },
      safeUrl: (raw) => safeUrl(raw, this.#allowHttp),
    };
  }

  unknown(surface: string, type: string): void {
    if (this.#reported.has(type)) return;
    this.#reported.add(type);
    this.#error(surface, "unknown_component", type);
  }

  failed(surface: string, id: string, error: unknown): void {
    this.#error(surface, "render_failed", `${id}: ${errorDetail(error)}`);
  }

  confirm(surface: string, op: ConfirmOp): void {
    this.#confirms.get(surface)?.close();
    const handle = openConfirm(op, {
      labels: this.#labels,
      delayMs: this.#confirmDelayMs,
      onConfirm: () => {
        this.#confirms.delete(surface);
        const message = this.#call(surface, () => this.#core.confirm(surface, op.action_id, op.nonce));
        if (message) this.#send(message);
      },
      onDismiss: () => {
        this.#confirms.delete(surface);
      },
    });
    this.#confirms.set(surface, handle);
  }

  dismiss(surface: string): void {
    this.#confirms.get(surface)?.close();
    this.#confirms.delete(surface);
  }

  #call(surface: string, run: () => string | null): string | null {
    try {
      return run();
    } catch (error) {
      this.#error(surface, "bad_message", errorDetail(error));
      return null;
    }
  }

  #accept(raw: string | null): void {
    if (raw === null || raw === "" || raw === "null") return;
    const batch = asBatch(parse(raw));
    if (!batch) {
      this.#error("", "bad_message", "core returned an unreadable batch");
      return;
    }
    for (const message of batch.send ?? []) this.#transmit(message);
    if (batch.ops.length > 0) this.#enqueue(batch);
  }

  #enqueue(batch: RenderBatch): void {
    if (this.#destroyed) return;
    this.#queue.push(batch);
    if (this.#sync) this.flush();
    else if (!this.#frame) this.#frame = requestAnimationFrame(() => this.flush());
  }

  #transmit(message: string | { readonly [key: string]: Json }): void {
    this.#send(typeof message === "string" ? message : JSON.stringify(message));
  }

  #error(surface: string, reason: string, detail: string): void {
    this.#transmit({ v: 1, t: "ui.error", surface, reason, detail });
  }
}

export function createRenderer(options: RendererOptions): Renderer {
  return new Renderer(options);
}

export function mount(container: Element, options: Omit<RendererOptions, "container">): Renderer {
  return new Renderer({ ...options, container });
}
