import { describe, expect, it, vi } from "vitest";
import { createRenderer } from "../src/index";
import { FakeCore } from "./fake-core";
import { billsBatch, harness } from "./setup";

describe("render ops", () => {
  it("creates components, places children and sets the root", () => {
    const h = harness();
    h.play(billsBatch);
    const surface = h.find("polyui-surface");
    expect(surface.getAttribute("surface")).toBe("s1");
    const card = h.find('[data-pui-id="card"]');
    expect(surface.firstElementChild).toBe(card);
    expect(card.getAttribute("role")).toBe("group");
    expect(card.getAttribute("aria-label")).toBe("Your bills");
    expect(card.querySelector(".pui-card-title")?.textContent).toBe("Your bills");
    const body = card.querySelector(".pui-card-body");
    expect([...(body?.children ?? [])].map((node) => node.getAttribute("data-pui-id"))).toEqual(["total", "phone", "plan", "pay"]);
    expect(h.find('[data-pui-id="total"]').getAttribute("role")).toBe("status");
    expect(h.find('[data-pui-id="pay"]').tagName).toBe("BUTTON");
    expect(h.find('[data-pui-id="pay"]').hasAttribute("role")).toBe(false);
  });

  it("updates in place without recreating nodes", () => {
    const h = harness();
    h.play(billsBatch);
    const stat = h.find('[data-pui-id="total"]');
    const value = h.find('[data-pui-id="total"] .pui-stat-value');
    const button = h.find('[data-pui-id="pay"]');
    h.play({
      surface: "s1",
      ops: [
        { op: "update", id: "total", props: { label: "Total", value: "41 722", unit: "so'm", tone: "positive" }, a11y: { role: "status", label: "Total: 41 722 so'm" } },
        { op: "update", id: "pay", props: { label: "Paid", tone: "secondary", on_press: { action_id: "act_pay", confirm: true }, disabled: true } },
      ],
    });
    expect(h.find('[data-pui-id="total"]')).toBe(stat);
    expect(h.find('[data-pui-id="total"] .pui-stat-value')).toBe(value);
    expect(value.textContent).toBe("41 722");
    expect(stat.getAttribute("data-tone")).toBe("positive");
    expect(stat.getAttribute("aria-label")).toBe("Total: 41 722 so'm");
    expect(h.find('[data-pui-id="pay"]')).toBe(button);
    expect((button as HTMLButtonElement).disabled).toBe(true);
    expect(button.textContent).toBe("Paid");
  });

  it("reorders children by moving only nodes that are out of place", () => {
    const h = harness();
    h.play(billsBatch);
    const pay = h.find('[data-pui-id="pay"]');
    const total = h.find('[data-pui-id="total"]');
    h.play({ surface: "s1", ops: [{ op: "children", id: "card", children: ["pay", "total"] }] });
    const body = h.find(".pui-card-body");
    expect([...body.children]).toEqual([pay, total]);
    expect(document.querySelector('[data-pui-id="phone"]')).toBeNull();
  });

  it("wraps List children in list items and drops the wrapper on remove", () => {
    const h = harness();
    h.play({
      surface: "s2",
      ops: [
        { op: "create", id: "list", type: "List", props: { title: "Stores", ordered: true } },
        { op: "create", id: "a", type: "Text", props: { text: "Korzinka", style: "body" } },
        { op: "create", id: "b", type: "Text", props: { text: "Makro", style: "body" } },
        { op: "children", id: "list", children: ["a", "b"] },
        { op: "root", id: "list" },
      ],
    });
    const list = h.find("ol.pui-items");
    expect(list.children).toHaveLength(2);
    expect(list.children[0]?.tagName).toBe("LI");
    h.play({ surface: "s2", ops: [{ op: "remove", id: "a" }] });
    expect(list.children).toHaveLength(1);
    expect(list.textContent).toBe("Makro");
    h.play({ surface: "s2", ops: [{ op: "update", id: "list", props: { title: "Stores", ordered: false } }] });
    expect(document.querySelector("ol.pui-items")).toBeNull();
    expect(h.find("ul.pui-items").textContent).toBe("Makro");
  });

  it("removes components and clears the root", () => {
    const h = harness();
    h.play(billsBatch);
    h.play({ surface: "s1", ops: [{ op: "remove", id: "pay" }] });
    expect(document.querySelector('[data-pui-id="pay"]')).toBeNull();
    h.play({ surface: "s1", ops: [{ op: "update", id: "pay", props: { label: "ghost" } }] });
    expect(document.querySelector('[data-pui-id="pay"]')).toBeNull();
  });

  it("closes a surface and empties it", () => {
    const h = harness();
    h.play(billsBatch);
    h.play({ surface: "s1", ops: [{ op: "close" }] });
    const surface = h.find("polyui-surface");
    expect(surface.childElementCount).toBe(0);
    expect(surface.hasAttribute("data-pui-closed")).toBe(true);
  });

  it("highlights the spoken component and moves the highlight", () => {
    const h = harness();
    h.play(billsBatch);
    h.play({ surface: "s1", ops: [{ op: "highlight", id: "total" }] });
    expect(h.find('[data-pui-id="total"]').hasAttribute("data-pui-speaking")).toBe(true);
    h.renderer.highlight("s1", "plan");
    expect(h.find('[data-pui-id="total"]').hasAttribute("data-pui-speaking")).toBe(false);
    expect(h.find('[data-pui-id="plan"]').hasAttribute("data-pui-speaking")).toBe(true);
    h.renderer.highlight("s1", null);
    expect(document.querySelector("[data-pui-speaking]")).toBeNull();
  });

  it("marks only top level new nodes for the entrance animation", () => {
    const h = harness();
    h.play(billsBatch);
    expect(h.find('[data-pui-id="card"]').hasAttribute("data-pui-enter")).toBe(true);
    expect(h.find('[data-pui-id="pay"]').hasAttribute("data-pui-enter")).toBe(false);
  });

  it("transmits messages carried in a batch", () => {
    const h = harness();
    h.core.receive = (message: string): string => {
      expect(message).toContain("ui.patch");
      return JSON.stringify({ surface: "s1", ops: [], send: [{ v: 1, t: "ui.resync", surface: "s1" }, '{"v":1,"t":"ui.error"}'] });
    };
    h.renderer.receive({ v: 1, t: "ui.patch", surface: "s1", seq: 9 });
    expect(h.messages()).toEqual([{ v: 1, t: "ui.resync", surface: "s1" }, { v: 1, t: "ui.error" }]);
  });

  it("applies a whole batch in one animation frame when not synchronous", async () => {
    const frames = vi.spyOn(window, "requestAnimationFrame");
    const core = new FakeCore();
    const container = document.createElement("div");
    document.body.replaceChildren(container);
    const renderer = createRenderer({ core, send: () => {}, container });
    renderer.receive(JSON.stringify(billsBatch));
    renderer.receive(JSON.stringify({ surface: "s1", ops: [{ op: "update", id: "total", props: { label: "Total", value: "1", unit: "so'm" } }] }));
    expect(container.querySelector('[data-pui-id="card"]')).toBeNull();
    expect(frames).toHaveBeenCalledTimes(1);
    await new Promise((resolve) => requestAnimationFrame(resolve));
    expect(container.querySelector('[data-pui-id="total"] .pui-stat-value')?.textContent).toBe("1");
    frames.mockRestore();
    renderer.destroy();
    expect(container.childElementCount).toBe(0);
  });
});

describe("unknown and failing components", () => {
  it("draws unknown types as their alt text and reports each type once", () => {
    const h = harness();
    h.play({
      surface: "s1",
      ops: [
        { op: "create", id: "x1", type: "acme:Mystery", props: { secret: "<img src=x onerror=alert(1)>" }, a11y: { label: "Mystery widget" } },
        { op: "create", id: "x2", type: "acme:Mystery", props: {}, a11y: { label: "Second" } },
        { op: "create", id: "row", type: "Row", props: {} },
        { op: "children", id: "row", children: ["x1", "x2"] },
        { op: "root", id: "row" },
      ],
    });
    const first = h.find('[data-pui-id="x1"]');
    expect(first.textContent).toBe("Mystery widget");
    expect(first.querySelector("img")).toBeNull();
    const errors = h.messages().filter((message) => message["t"] === "ui.error");
    expect(errors).toEqual([{ v: 1, t: "ui.error", surface: "s1", reason: "unknown_component", detail: "acme:Mystery" }]);
  });

  it("uses a host renderer for extension types", () => {
    const h = harness();
    h.renderer.define("acme:InvoiceCard", (api) => {
      const node = document.createElement("article");
      node.className = "invoice";
      node.addEventListener("click", () => api.fire("on_open"));
      return {
        element: node,
        update({ props }) {
          node.textContent = `Invoice ${String(props["number"])}`;
        },
      };
    });
    h.play({
      surface: "s1",
      ops: [
        { op: "create", id: "inv", type: "acme:InvoiceCard", props: { number: 42, on_open: { action_id: "act_open" } }, a11y: { role: "group", label: "Invoice 42" } },
        { op: "root", id: "inv" },
      ],
    });
    const invoice = h.find(".invoice");
    expect(invoice.textContent).toBe("Invoice 42");
    expect(invoice.getAttribute("role")).toBe("group");
    invoice.click();
    expect(h.messages()[0]).toMatchObject({ t: "ui.action", action_id: "act_open" });
  });

  it("survives a renderer that throws and reports render_failed", () => {
    const h = harness();
    h.renderer.define("acme:Broken", () => {
      throw new Error("boom");
    });
    h.play({ surface: "s1", ops: [{ op: "create", id: "b", type: "acme:Broken", a11y: { label: "Broken thing" } }, { op: "root", id: "b" }] });
    expect(h.find('[data-pui-id="b"]').textContent).toBe("Broken thing");
    expect(h.messages()[0]).toMatchObject({ t: "ui.error", reason: "render_failed" });
  });

  it("ignores ops for ids it does not know and bad batches", () => {
    const h = harness();
    h.play({ surface: "s1", ops: [{ op: "children", id: "nope", children: ["a"] }, { op: "root", id: "nope" }, { op: "remove", id: "nope" }] });
    expect(h.find("polyui-surface").childElementCount).toBe(0);
    h.core.receive = () => "{not json";
    h.renderer.receive("{}");
    expect(h.messages().at(-1)).toMatchObject({ t: "ui.error", reason: "bad_message" });
  });
});

describe("voice", () => {
  it("returns utterances and applies voice matches", () => {
    const h = harness();
    h.play(billsBatch);
    expect(h.renderer.say("s1").map((item) => item.id)).toEqual(["card", "total", "phone", "plan", "pay"]);
    const match = h.renderer.matchVoice("s1", "ikkinchisini tanla");
    expect(match?.option).toBe("power");
    expect(h.find<HTMLInputElement>('input[value="power"]').checked).toBe(true);
    expect(h.messages().at(-1)).toMatchObject({ t: "ui.action", action_id: "act_plan" });
    expect(h.renderer.matchVoice("s1", "nothing here")).toBeNull();
  });
});
