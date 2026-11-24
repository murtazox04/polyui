import { createRenderer } from "../src/index";
import type { Renderer } from "../src/renderer";
import type { RenderBatch, RendererOptions } from "../src/types";
import { FakeCore } from "./fake-core";

export interface Harness {
  readonly core: FakeCore;
  readonly renderer: Renderer;
  readonly container: HTMLElement;
  readonly sent: string[];
  play(batch: RenderBatch): void;
  find<T extends Element = HTMLElement>(selector: string): T;
  messages(): { [key: string]: unknown }[];
}

export function harness(options: Partial<RendererOptions> = {}): Harness {
  document.body.replaceChildren();
  const container = document.createElement("div");
  document.body.append(container);
  const core = new FakeCore();
  const sent: string[] = [];
  const renderer = createRenderer({ core, send: (message) => sent.push(message), container, sync: true, confirmDelayMs: 0, ...options });
  return {
    core,
    renderer,
    container,
    sent,
    play: (batch) => renderer.receive(JSON.stringify(batch)),
    find: <T extends Element = HTMLElement>(selector: string): T => {
      const found = document.querySelector<T>(selector);
      if (!found) throw new Error(`missing ${selector}`);
      return found;
    },
    messages: () => sent.map((message) => JSON.parse(message) as { [key: string]: unknown }),
  };
}

export const billsBatch: RenderBatch = {
  surface: "s1",
  ops: [
    { op: "create", id: "card", type: "Card", props: { title: "Your bills", tone: "plain" }, a11y: { role: "group", label: "Your bills" } },
    { op: "create", id: "total", type: "Stat", props: { label: "Total", value: "29 322", unit: "so'm", tone: "negative" }, a11y: { role: "status", label: "Total: 29 322 so'm" } },
    { op: "create", id: "phone", type: "Input", props: { label: "Phone", value: "/form/phone", input: "phone", max_length: 20, required: true }, a11y: { role: "textbox", label: "Phone" }, value: "" },
    {
      op: "create",
      id: "plan",
      type: "ChoiceGroup",
      props: {
        label: "Pick a plan",
        options: [
          { id: "gas", label: "Gas", detail: "29 322 so'm" },
          { id: "power", label: "Power", detail: "12 400 so'm" },
          { id: "water", label: "Water" },
        ],
        multiple: false,
        value: "/form/plan",
        on_change: { action_id: "act_plan", confirm: false },
      },
      a11y: { role: "radiogroup", label: "Pick a plan" },
      value: "gas",
    },
    { op: "create", id: "pay", type: "Button", props: { label: "Pay", tone: "primary", on_press: { action_id: "act_pay", confirm: true }, disabled: false }, a11y: { role: "button", label: "Pay" } },
    { op: "children", id: "card", children: ["total", "phone", "plan", "pay"] },
    { op: "root", id: "card" },
  ],
};
