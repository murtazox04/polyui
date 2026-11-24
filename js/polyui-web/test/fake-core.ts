import type { A11y, Json, PolyuiCore, Props, RenderBatch, RenderOp } from "../src/types";

interface FakeComponent {
  type: string;
  props: Props;
  a11y: A11y | undefined;
  children: string[];
}

interface FakeSurface {
  root: string | null;
  components: Map<string, FakeComponent>;
  data: Map<string, Json>;
}

export interface FakeCall {
  readonly method: "receive" | "setValue" | "fire" | "confirm" | "say" | "matchVoice";
  readonly args: readonly string[];
}

const ordinals: Record<string, number> = {
  first: 0,
  second: 1,
  third: 2,
  birinchi: 0,
  ikkinchi: 1,
  uchinchi: 2,
};

function record(value: Json | undefined): { [key: string]: Json } | null {
  return typeof value === "object" && value !== null && !Array.isArray(value) ? value : null;
}

export class FakeCore implements PolyuiCore {
  readonly calls: FakeCall[] = [];
  #surfaces = new Map<string, FakeSurface>();

  receive(message: string): string {
    this.calls.push({ method: "receive", args: [message] });
    const parsed = JSON.parse(message) as Json;
    const value = record(parsed);
    if (!value) return "null";
    if (value["t"] === "ui.confirm") {
      const surface = String(value["surface"]);
      const op: RenderOp = {
        op: "confirm",
        action_id: String(value["action_id"]),
        nonce: String(value["nonce"]),
        title: String(value["title"] ?? ""),
        body: String(value["body"] ?? ""),
        confirm_label: String(value["confirm_label"] ?? "OK"),
        expires_ms: typeof value["expires_ms"] === "number" ? value["expires_ms"] : 60000,
      };
      return JSON.stringify({ surface, ops: [op] } satisfies RenderBatch);
    }
    if (typeof value["surface"] !== "string" || !Array.isArray(value["ops"])) {
      return JSON.stringify({ surface: String(value["surface"] ?? ""), ops: [] });
    }
    const batch = value as unknown as RenderBatch;
    this.#track(batch);
    return message;
  }

  setValue(surface: string, path: string, value: string): string {
    this.calls.push({ method: "setValue", args: [surface, path, value] });
    const state = this.#surface(surface);
    const parsed = JSON.parse(value) as Json;
    state.data.set(path, parsed);
    const ops: RenderOp[] = [];
    for (const [id, component] of state.components) {
      if (component.props["value"] === path) {
        ops.push({ op: "update", id, props: component.props, ...(component.a11y ? { a11y: component.a11y } : {}), value: parsed });
      }
    }
    return JSON.stringify({ surface, ops } satisfies RenderBatch);
  }

  fire(surface: string, component: string, prop: string): string | null {
    this.calls.push({ method: "fire", args: [surface, component, prop] });
    const found = this.#surface(surface).components.get(component);
    const [name = "", row] = prop.split(":");
    const handle = record(found?.props[name]);
    if (!found || !handle || found.props["disabled"] === true) return null;
    const values: { [key: string]: Json } = Object.fromEntries(this.#surface(surface).data);
    if (row !== undefined) values["row"] = Number(row);
    return JSON.stringify({ v: 1, t: "ui.action", surface, action_id: handle["action_id"] ?? null, values, source: "tap" });
  }

  confirm(surface: string, actionId: string, nonce: string): string {
    this.calls.push({ method: "confirm", args: [surface, actionId, nonce] });
    return JSON.stringify({ v: 1, t: "ui.action", surface, action_id: actionId, confirm: nonce, source: "tap" });
  }

  say(surface: string): string {
    this.calls.push({ method: "say", args: [surface] });
    const state = this.#surface(surface);
    const spoken: { id: string; text: string }[] = [];
    const walk = (id: string): void => {
      const component = state.components.get(id);
      if (!component) return;
      if (component.a11y?.label) spoken.push({ id, text: component.a11y.label });
      for (const child of component.children) walk(child);
    };
    if (state.root) walk(state.root);
    return JSON.stringify(spoken);
  }

  matchVoice(surface: string, text: string): string | null {
    this.calls.push({ method: "matchVoice", args: [surface, text] });
    const state = this.#surface(surface);
    const word = Object.keys(ordinals).find((key) => text.toLowerCase().includes(key));
    if (word === undefined) return null;
    for (const [id, component] of state.components) {
      if (component.type !== "ChoiceGroup") continue;
      const options = component.props["options"];
      const option = Array.isArray(options) ? record(options[ordinals[word] ?? -1]) : null;
      const path = component.props["value"];
      if (!option || typeof path !== "string") return null;
      const batch = JSON.parse(this.setValue(surface, path, JSON.stringify(option["id"] ?? null))) as Json;
      const action = record(component.props["on_change"]) ? (JSON.parse(this.fire(surface, id, "on_change") ?? "null") as Json) : null;
      return JSON.stringify({ component: id, option: option["id"] ?? null, batch, action });
    }
    return null;
  }

  dataOf(surface: string, path: string): Json | undefined {
    return this.#surface(surface).data.get(path);
  }

  #surface(id: string): FakeSurface {
    let state = this.#surfaces.get(id);
    if (!state) {
      state = { root: null, components: new Map(), data: new Map() };
      this.#surfaces.set(id, state);
    }
    return state;
  }

  #track(batch: RenderBatch): void {
    const state = this.#surface(batch.surface);
    for (const op of batch.ops) {
      if (op.op === "create") {
        state.components.set(op.id, { type: op.type, props: op.props ?? {}, a11y: op.a11y, children: [] });
        if (op.value !== undefined && typeof op.props?.["value"] === "string") state.data.set(op.props["value"], op.value);
      } else if (op.op === "update") {
        const component = state.components.get(op.id);
        if (component) {
          component.props = op.props ?? {};
          component.a11y = op.a11y;
        }
      } else if (op.op === "children") {
        const component = state.components.get(op.id);
        if (component) component.children = [...op.children];
      } else if (op.op === "remove") {
        state.components.delete(op.id);
      } else if (op.op === "root") {
        state.root = op.id;
      } else if (op.op === "close") {
        state.components.clear();
        state.root = null;
      }
    }
  }
}
