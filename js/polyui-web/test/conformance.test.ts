import { readFileSync, readdirSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import { loadCore, type WasmCore } from "../src/core";
import { isRecord } from "../src/dom";
import { createRenderer, type Renderer } from "../src/renderer";
import type { A11y, ConfirmOp, Json, PolyuiCore, Props, RenderOp } from "../src/types";

type JsonObject = { readonly [key: string]: Json };

const fixtures = join(import.meta.dirname, "..", "..", "..", "tests", "conformance", "client");
const wasmBytes = readFileSync(join(import.meta.dirname, "..", "wasm", "polyui_core_bg.wasm"));
const nativeSemantics = new Set(["Text", "List", "Image", "Button", "ChoiceGroup", "Input", "ConfirmSheet", "Divider", "chart:Chart"]);
const exposedRoles = new Set(["group", "list", "listitem", "button", "radiogroup", "img", "textbox", "dialog", "figure", "status"]);

interface Fixture {
  readonly name: string;
  readonly hello: JsonObject;
  readonly steps: readonly JsonObject[];
}

interface Expected {
  type: string;
  props: Props;
  a11y: A11y | undefined;
  value: Json | undefined;
  children: string[];
}

interface Model {
  readonly nodes: Map<string, Expected>;
  root: string | null;
  highlight: string | null;
  confirm: ConfirmOp | null;
  closed: boolean;
}

class RecordingCore implements PolyuiCore {
  readonly outputs: (string | null)[] = [];
  readonly #inner: WasmCore;

  constructor(inner: WasmCore) {
    this.#inner = inner;
  }

  receive(message: string): string {
    return this.#keep(this.#inner.receive(message));
  }

  setValue(surface: string, path: string, value: string): string {
    return this.#keep(this.#inner.setValue(surface, path, value));
  }

  fire(surface: string, component: string, prop: string, detail?: string): string | null {
    return this.#keep(this.#inner.fire(surface, component, prop, detail));
  }

  confirm(surface: string, actionId: string, nonce: string): string | null {
    return this.#keep(this.#inner.confirm(surface, actionId, nonce));
  }

  say(surface: string): string {
    return this.#keep(this.#inner.say(surface));
  }

  matchVoice(surface: string, text: string): string | null {
    return this.#keep(this.#inner.matchVoice(surface, text));
  }

  last(): Json {
    const output = this.outputs.at(-1);
    return output === null || output === undefined ? null : (JSON.parse(output) as Json);
  }

  #keep<T extends string | null>(output: T): T {
    this.outputs.push(output);
    return output;
  }
}

function readFixtures(): Fixture[] {
  return readdirSync(fixtures)
    .filter((name) => name.endsWith(".json"))
    .sort()
    .map((name) => {
      const parsed = JSON.parse(readFileSync(join(fixtures, name), "utf8")) as Json;
      if (!isRecord(parsed) || !isRecord(parsed["hello"]) || !Array.isArray(parsed["steps"])) throw new Error(`${name} is not a client fixture`);
      return { name, hello: parsed["hello"], steps: parsed["steps"].filter(isRecord) };
    });
}

function record(value: Json | undefined): JsonObject {
  return isRecord(value) ? value : {};
}

function text(value: Json | undefined): string {
  return typeof value === "string" ? value : "";
}

function opsOf(batch: Json | undefined): RenderOp[] {
  const ops = record(batch)["ops"];
  return Array.isArray(ops) ? (ops as unknown as RenderOp[]) : [];
}

function sendOf(batch: Json | undefined): Json[] {
  const send = record(batch)["send"];
  return Array.isArray(send) ? send : [];
}

function applyOps(model: Model, ops: readonly RenderOp[]): void {
  for (const op of ops) {
    switch (op.op) {
      case "create":
        model.closed = false;
        model.nodes.set(op.id, { type: op.type, props: op.props ?? {}, a11y: op.a11y, value: op.value, children: [] });
        break;
      case "update": {
        const node = model.nodes.get(op.id);
        if (node) Object.assign(node, { props: op.props ?? {}, a11y: op.a11y, value: op.value });
        break;
      }
      case "children": {
        const node = model.nodes.get(op.id);
        if (node) node.children = [...op.children];
        break;
      }
      case "remove":
        model.nodes.delete(op.id);
        if (model.root === op.id) model.root = null;
        if (model.highlight === op.id) model.highlight = null;
        break;
      case "root":
        model.root = op.id;
        break;
      case "confirm":
        model.confirm = op;
        break;
      case "highlight":
        model.highlight = op.id;
        break;
      case "close":
        model.nodes.clear();
        model.root = null;
        model.highlight = null;
        model.confirm = null;
        model.closed = true;
        break;
    }
  }
}

function reachable(model: Model): string[] {
  const order: string[] = [];
  const visit = (id: string): void => {
    const node = model.nodes.get(id);
    if (!node || order.includes(id)) return;
    order.push(id);
    for (const child of node.children) visit(child);
  };
  if (model.root !== null) visit(model.root);
  return order;
}

function selected(value: Json | undefined): string[] {
  if (typeof value === "string") return [value];
  if (Array.isArray(value)) return value.filter((item): item is string => typeof item === "string");
  return [];
}

function display(value: Json | undefined): string {
  if (typeof value === "string") return value;
  if (typeof value === "number") return String(value);
  return "";
}

function checkContent(element: Element, node: Expected): void {
  const props = node.props;
  switch (node.type) {
    case "Text":
      expect(element.textContent).toBe(text(props["text"]));
      break;
    case "Button":
      expect(element.querySelector(".pui-button-label")?.textContent).toBe(text(props["label"]));
      expect((element as HTMLButtonElement).disabled).toBe(props["disabled"] === true || !isRecord(props["on_press"]));
      break;
    case "Input":
      expect(element.querySelector("label")?.textContent).toBe(text(props["label"]));
      expect(element.querySelector<HTMLInputElement>(".pui-control")?.value).toBe(display(node.value));
      break;
    case "ChoiceGroup": {
      const checked = [...element.querySelectorAll<HTMLInputElement>("input")].filter((input) => input.checked).map((input) => input.value);
      expect(checked).toEqual(selected(node.value));
      expect(element.querySelector("legend")?.textContent).toBe(text(props["label"]));
      break;
    }
    case "Card":
      expect(element.querySelector(".pui-card-title")?.textContent).toBe(text(props["title"]));
      break;
    case "Stat":
      expect(element.querySelector(".pui-stat-value")?.textContent).toBe(text(props["value"]));
      break;
    case "chart:Chart": {
      expect(element.querySelector("figcaption")?.textContent).toBe(text(props["title"]));
      const drawable = isRecord(record(props["scene"])["view_box"]);
      expect(element.querySelector("svg") !== null).toBe(drawable);
      expect(element.querySelector(".pui-chart-empty") !== null).toBe(!drawable);
      break;
    }
  }
  if (nativeSemantics.has(node.type)) return;
  const role = node.a11y?.role;
  expect(element.getAttribute("role")).toBe(typeof role === "string" && exposedRoles.has(role) ? role : null);
  const label = node.a11y?.label;
  expect(element.getAttribute("aria-label")).toBe(typeof label === "string" && label !== "" ? label : null);
}

function checkDom(container: HTMLElement, surface: string, model: Model): void {
  const host = container.querySelector(`polyui-surface[surface="${surface}"]`);
  if (!host) {
    expect(model.nodes.size).toBe(0);
    return;
  }
  if (model.closed) {
    expect(host.hasAttribute("data-pui-closed")).toBe(true);
    expect(host.childElementCount).toBe(0);
    return;
  }
  const find = (id: string): Element => {
    const found = host.querySelector(`[data-pui-id="${id}"]`);
    if (!found) throw new Error(`component ${id} is not in the DOM`);
    return found;
  };
  const ids = reachable(model);
  for (const id of ids) {
    const node = model.nodes.get(id);
    if (!node) continue;
    const element = find(id);
    expect(element.getAttribute("data-pui-type")).toBe(node.type);
    checkContent(element, node);
    const kids = node.children.filter((child) => model.nodes.has(child)).map(find);
    for (const kid of kids) expect(element.contains(kid)).toBe(true);
    for (let index = 1; index < kids.length; index += 1) {
      const before = kids[index - 1];
      const after = kids[index];
      if (before && after) expect(before.compareDocumentPosition(after) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    }
  }
  if (model.root !== null) expect(host.firstElementChild).toBe(find(model.root));
  expect(host.querySelector("[data-pui-speaking]")?.getAttribute("data-pui-id") ?? null).toBe(model.highlight);
}

function confirmDialog(): HTMLDialogElement | null {
  return document.querySelector<HTMLDialogElement>("dialog.pui-confirm");
}

describe("conformance fixtures through the wasm core and the renderer", () => {
  it.each(readFixtures().map((fixture) => [fixture.name, fixture] as const))("%s", async (_, fixture) => {
    document.body.replaceChildren();
    const container = document.createElement("div");
    document.body.append(container);
    const wasm = await loadCore(fixture.hello, wasmBytes);
    const core = new RecordingCore(wasm);
    const sent: Json[] = [];
    const renderer: Renderer = createRenderer({ core, send: (message) => sent.push(JSON.parse(message) as Json), container, sync: true, confirmDelayMs: 0 });
    const models = new Map<string, Model>();
    const model = (surface: string): Model => {
      let found = models.get(surface);
      if (!found) {
        found = { nodes: new Map(), root: null, highlight: null, confirm: null, closed: false };
        models.set(surface, found);
      }
      return found;
    };

    for (const step of fixture.steps) {
      const expected = step["expect"];
      const sentBefore = sent.length;
      const expectSent = (messages: readonly Json[]): void => expect(sent.slice(sentBefore)).toEqual(messages);

      if (step["receive"] !== undefined || step["receive_text"] !== undefined) {
        const raw = step["receive_text"];
        renderer.receive(typeof raw === "string" ? raw : JSON.stringify(step["receive"]));
        expect(core.last()).toEqual(expected);
        const surface = text(record(expected)["surface"]);
        applyOps(model(surface), opsOf(expected));
        expectSent(sendOf(expected));
        checkDom(container, surface, model(surface));
      } else if (isRecord(step["set_value"])) {
        const { surface, path, value } = step["set_value"];
        const target = [...model(text(surface)).nodes.entries()].find(([, node]) => node.type === "Input" && node.props["value"] === path);
        const control = target ? container.querySelector<HTMLInputElement>(`[data-pui-id="${target[0]}"] .pui-control`) : null;
        if (control && typeof value === "string") {
          control.value = value;
          control.dispatchEvent(new Event("input", { bubbles: true }));
        } else {
          renderer.api(text(surface), "", "").setValue(text(path), value ?? null);
        }
        expect(core.last()).toEqual(expected);
        applyOps(model(text(surface)), opsOf(expected));
        expectSent(sendOf(expected));
        checkDom(container, text(surface), model(text(surface)));
      } else if (isRecord(step["fire"])) {
        const { surface, component, prop, detail } = step["fire"];
        const node = model(text(surface)).nodes.get(text(component));
        const button = container.querySelector<HTMLButtonElement>(`polyui-surface[surface="${text(surface)}"] button[data-pui-id="${text(component)}"]`);
        if (button && node?.type === "Button" && prop === "on_press" && detail === undefined) button.click();
        else renderer.api(text(surface), text(component), node?.type ?? "").fire(text(prop), detail);
        expect(core.last()).toEqual(expected);
        expectSent(expected === null || expected === undefined ? [] : [expected]);
      } else if (isRecord(step["confirm"])) {
        const { surface, action_id: actionId, nonce } = step["confirm"];
        if (!confirmDialog()) renderer.confirm(text(surface), { op: "confirm", action_id: text(actionId), nonce: text(nonce), title: "" });
        const dialog = confirmDialog();
        const pending = model(text(surface)).confirm;
        if (pending) expect(dialog?.querySelector(".pui-confirm-title")?.textContent).toBe(pending.title);
        dialog?.querySelector<HTMLButtonElement>('.pui-button[data-tone="primary"]')?.click();
        model(text(surface)).confirm = null;
        expect(confirmDialog()).toBeNull();
        expect(core.last()).toEqual(expected);
        expectSent(expected === null || expected === undefined ? [] : [expected]);
      } else if (step["say"] !== undefined) {
        expect(renderer.say(text(step["say"]))).toEqual(expected);
      } else if (isRecord(step["match_voice"])) {
        const { surface, text: spoken } = step["match_voice"];
        const found = renderer.matchVoice(text(surface), text(spoken));
        expect(found).toEqual(expected);
        applyOps(model(text(surface)), opsOf(record(expected)["batch"]));
        const action = record(expected)["action"];
        expectSent(isRecord(action) ? [action] : []);
        checkDom(container, text(surface), model(text(surface)));
      } else {
        throw new Error(`${fixture.name} has a step this test does not know: ${Object.keys(step).join(", ")}`);
      }
    }
    renderer.destroy();
    wasm.free();
  });
});
