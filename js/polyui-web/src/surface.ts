import { nativeSemantics, renderFallback } from "./components";
import { setAttr, syncChildren } from "./dom";
import type {
  A11y,
  ComponentApi,
  ComponentRenderer,
  ComponentView,
  ConfirmOp,
  CreateOp,
  Json,
  Props,
  RenderBatch,
  RenderOp,
} from "./types";

export interface SurfaceHost {
  resolve(type: string): ComponentRenderer | undefined;
  api(surface: string, id: string, type: string): ComponentApi;
  unknown(surface: string, type: string): void;
  failed(surface: string, id: string, error: unknown): void;
  confirm(surface: string, op: ConfirmOp): void;
  dismiss(surface: string): void;
}

interface Entry {
  readonly type: string;
  readonly view: ComponentView;
  readonly semantic: boolean;
}

interface StateOp {
  readonly id: string;
  readonly props?: Props;
  readonly a11y?: A11y;
  readonly value?: Json;
}

const exposedRoles = new Set(["group", "list", "listitem", "button", "radiogroup", "img", "textbox", "dialog", "figure", "status"]);

export const surfaceTag = "polyui-surface";

export class PolyuiSurface extends HTMLElement {
  #entries = new Map<string, Entry>();
  #host: SurfaceHost | null = null;
  #speaking: Element | null = null;
  #rootId: string | null = null;

  constructor() {
    super();
    this.addEventListener("animationend", (event) => {
      const target = event.target;
      if (target instanceof Element && event.animationName.startsWith("pui-enter")) target.removeAttribute("data-pui-enter");
    });
  }

  get surface(): string {
    return this.getAttribute("surface") ?? "";
  }

  get root(): string | null {
    return this.#rootId;
  }

  attachHost(host: SurfaceHost): void {
    this.#host = host;
  }

  component(id: string): Element | null {
    return this.#entries.get(id)?.view.element ?? null;
  }

  apply(batch: RenderBatch): void {
    const created: Element[] = [];
    for (const op of batch.ops) this.#applyOp(op, created);
    for (const node of created) {
      if (node.parentElement?.closest("[data-pui-enter]")) node.removeAttribute("data-pui-enter");
    }
  }

  dispose(): void {
    for (const entry of this.#entries.values()) entry.view.destroy?.();
    this.#entries.clear();
    this.#speaking = null;
    this.#rootId = null;
    this.replaceChildren();
  }

  #applyOp(op: RenderOp, created: Element[]): void {
    switch (op.op) {
      case "create":
        if (typeof op.id === "string" && typeof op.type === "string") this.#create(op, created);
        return;
      case "update": {
        const entry = this.#entries.get(op.id);
        if (entry) this.#render(entry, op);
        return;
      }
      case "children": {
        const view = this.#entries.get(op.id)?.view;
        if (!view?.setChildren || !Array.isArray(op.children)) return;
        const children: Element[] = [];
        for (const id of op.children) {
          const child = this.#entries.get(id);
          if (child && child.view !== view) children.push(child.view.element);
        }
        view.setChildren(children);
        return;
      }
      case "remove":
        this.#remove(op.id);
        return;
      case "root": {
        const entry = this.#entries.get(op.id);
        if (!entry) return;
        this.#rootId = op.id;
        syncChildren(this, [entry.view.element]);
        return;
      }
      case "confirm":
        this.#host?.confirm(this.surface, op);
        return;
      case "highlight": {
        this.#speaking?.removeAttribute("data-pui-speaking");
        this.#speaking = typeof op.id === "string" ? (this.component(op.id) ?? null) : null;
        this.#speaking?.setAttribute("data-pui-speaking", "");
        return;
      }
      case "close":
        this.#host?.dismiss(this.surface);
        this.dispose();
        this.setAttribute("data-pui-closed", "");
        return;
    }
  }

  #create(op: CreateOp, created: Element[]): void {
    const existing = this.#entries.get(op.id);
    if (existing && existing.type === op.type) {
      this.#render(existing, op);
      return;
    }
    this.removeAttribute("data-pui-closed");
    const entry = this.#build(op.id, op.type);
    entry.view.element.setAttribute("data-pui-id", op.id);
    entry.view.element.setAttribute("data-pui-type", op.type);
    entry.view.element.setAttribute("data-pui-enter", "");
    if (existing) {
      existing.view.destroy?.();
      existing.view.element.replaceWith(entry.view.element);
    }
    this.#entries.set(op.id, entry);
    created.push(entry.view.element);
    this.#render(entry, op);
  }

  #build(id: string, type: string): Entry {
    const host = this.#host;
    const renderer = host?.resolve(type);
    if (!host || !renderer) {
      host?.unknown(this.surface, type);
      return { type, view: renderFallback(""), semantic: true };
    }
    try {
      return { type, view: renderer(host.api(this.surface, id, type)), semantic: nativeSemantics.has(type) };
    } catch (error) {
      host.failed(this.surface, id, error);
      return { type, view: renderFallback(""), semantic: true };
    }
  }

  #render(entry: Entry, op: StateOp): void {
    try {
      entry.view.update({ props: op.props ?? {}, a11y: op.a11y, value: op.value });
    } catch (error) {
      this.#host?.failed(this.surface, op.id, error);
    }
    if (entry.semantic) return;
    const role = op.a11y?.role;
    setAttr(entry.view.element, "role", typeof role === "string" && exposedRoles.has(role) ? role : null);
    setAttr(entry.view.element, "aria-label", typeof op.a11y?.label === "string" ? op.a11y.label : null);
  }

  #remove(id: string): void {
    const entry = this.#entries.get(id);
    if (!entry) return;
    this.#entries.delete(id);
    entry.view.destroy?.();
    const node = entry.view.element;
    if (this.#speaking === node) this.#speaking = null;
    if (this.#rootId === id) this.#rootId = null;
    const wrapper = node.parentElement;
    node.remove();
    if (wrapper?.hasAttribute("data-pui-wrap")) wrapper.remove();
  }
}

export function defineSurfaceElement(): void {
  if (!customElements.get(surfaceTag)) customElements.define(surfaceTag, PolyuiSurface);
}
