import { afterEach, describe, expect, it, vi } from "vitest";
import { safeUrl } from "../src/index";
import { billsBatch, harness } from "./setup";

afterEach(() => {
  vi.useRealTimers();
});

describe("Input", () => {
  it("writes typing to the core and keeps focus and caret when its own update arrives", () => {
    const h = harness();
    h.play(billsBatch);
    const input = h.find<HTMLInputElement>('[data-pui-id="phone"] input');
    const label = h.find<HTMLLabelElement>('[data-pui-id="phone"] label');
    expect(label.htmlFor).toBe(input.id);
    expect(input.type).toBe("tel");
    expect(input.required).toBe(true);
    expect(input.maxLength).toBe(20);
    input.focus();
    input.value = "+99890";
    input.setSelectionRange(3, 3);
    input.dispatchEvent(new Event("input", { bubbles: true }));
    expect(h.core.calls.at(-1)).toEqual({ method: "setValue", args: ["s1", "/form/phone", '"+99890"'] });
    expect(h.find<HTMLInputElement>('[data-pui-id="phone"] input')).toBe(input);
    expect(document.activeElement).toBe(input);
    expect(input.selectionStart).toBe(3);
    expect(input.value).toBe("+99890");
  });

  it("ignores a stale echo of earlier typing while focused", () => {
    const h = harness();
    h.play(billsBatch);
    const input = h.find<HTMLInputElement>('[data-pui-id="phone"] input');
    input.focus();
    input.value = "+9";
    input.dispatchEvent(new Event("input", { bubbles: true }));
    input.value = "+99";
    input.dispatchEvent(new Event("input", { bubbles: true }));
    input.setSelectionRange(3, 3);
    const props = { label: "Phone", value: "/form/phone", input: "phone" };
    h.play({ surface: "s1", ops: [{ op: "update", id: "phone", props, value: "+9" }] });
    expect(input.value).toBe("+99");
    expect(input.selectionStart).toBe(3);
    h.play({ surface: "s1", ops: [{ op: "update", id: "phone", props, value: "+998" }] });
    expect(input.value).toBe("+998");
    expect(document.activeElement).toBe(input);
  });

  it("keeps focus when the focused field is moved by a reorder", () => {
    const h = harness();
    h.play(billsBatch);
    const input = h.find<HTMLInputElement>('[data-pui-id="phone"] input');
    input.focus();
    input.value = "abc";
    input.setSelectionRange(1, 2);
    h.play({ surface: "s1", ops: [{ op: "children", id: "card", children: ["pay", "plan", "phone", "total"] }] });
    expect(document.activeElement).toBe(input);
    expect([input.selectionStart, input.selectionEnd]).toEqual([1, 2]);
  });

  it("switches to a textarea for multiline and keeps the typed value", () => {
    const h = harness();
    h.play(billsBatch);
    const input = h.find<HTMLInputElement>('[data-pui-id="phone"] input');
    input.value = "note";
    h.play({ surface: "s1", ops: [{ op: "update", id: "phone", props: { label: "Note", value: "/form/phone", input: "multiline" }, value: "note" }] });
    const area = h.find<HTMLTextAreaElement>('[data-pui-id="phone"] textarea');
    expect(area.value).toBe("note");
    expect(area.id).toBe(input.id);
  });
});

describe("ChoiceGroup", () => {
  it("renders a fieldset of radios and reports a selection to the core", () => {
    const h = harness();
    h.play(billsBatch);
    const group = h.find('[data-pui-id="plan"]');
    expect(group.tagName).toBe("FIELDSET");
    expect(group.querySelector("legend")?.textContent).toBe("Pick a plan");
    const radios = [...group.querySelectorAll<HTMLInputElement>("input")];
    expect(radios.map((radio) => radio.type)).toEqual(["radio", "radio", "radio"]);
    expect(radios.map((radio) => radio.checked)).toEqual([true, false, false]);
    expect(new Set(radios.map((radio) => radio.name)).size).toBe(1);
    radios[1]?.click();
    expect(h.core.calls.filter((call) => call.method === "setValue").at(-1)?.args).toEqual(["s1", "/form/plan", '"power"']);
    expect(h.core.calls.at(-1)).toEqual({ method: "fire", args: ["s1", "plan", "on_change"] });
    expect(h.messages().at(-1)).toMatchObject({ t: "ui.action", action_id: "act_plan", values: { "/form/plan": "power" } });
    expect(radios.map((radio) => radio.checked)).toEqual([false, true, false]);
  });

  it("keeps option nodes and focus when options update", () => {
    const h = harness();
    h.play(billsBatch);
    const power = h.find<HTMLInputElement>('input[value="power"]');
    power.focus();
    h.play({
      surface: "s1",
      ops: [
        {
          op: "update",
          id: "plan",
          props: { label: "Pick", options: [{ id: "power", label: "Power+" }, { id: "gas", label: "Gas" }], value: "/form/plan", multiple: false },
          value: "power",
        },
      ],
    });
    expect(h.find<HTMLInputElement>('input[value="power"]')).toBe(power);
    expect(document.activeElement).toBe(power);
    expect(power.checked).toBe(true);
    expect(document.querySelector('input[value="water"]')).toBeNull();
    expect(h.find('[data-pui-id="plan"] .pui-option-label').textContent).toBe("Power+");
  });

  it("sends an array of ids when multiple is set", () => {
    const h = harness();
    h.play({
      surface: "s1",
      ops: [
        { op: "create", id: "c", type: "ChoiceGroup", props: { options: [{ id: "a", label: "A" }, { id: "b", label: "B" }], multiple: true, value: "/picks" }, value: ["b"] },
        { op: "root", id: "c" },
      ],
    });
    const boxes = [...document.querySelectorAll<HTMLInputElement>("input")];
    expect(boxes.map((box) => box.type)).toEqual(["checkbox", "checkbox"]);
    boxes[0]?.click();
    expect(h.core.calls.at(-1)).toEqual({ method: "setValue", args: ["s1", "/picks", '["a","b"]'] });
    expect(h.sent).toHaveLength(0);
  });
});

describe("Button and confirm", () => {
  it("fires on press and runs the server confirm flow in a renderer owned dialog", () => {
    const h = harness();
    h.play(billsBatch);
    h.find('[data-pui-id="pay"]').click();
    expect(h.core.calls.at(-1)).toEqual({ method: "fire", args: ["s1", "pay", "on_press"] });
    expect(h.messages().at(-1)).toMatchObject({ t: "ui.action", action_id: "act_pay" });
    h.renderer.receive({ v: 1, t: "ui.confirm", surface: "s1", action_id: "act_pay", nonce: "n_77ad", title: "Pay Hududgaz", body: "29 322,19 so'm from card 4471", confirm_label: "Pay", expires_ms: 60000 });
    const dialog = h.find<HTMLDialogElement>("dialog.pui-confirm");
    expect(dialog.parentElement).toBe(document.body);
    expect(h.find("polyui-surface").contains(dialog)).toBe(false);
    expect(dialog.open).toBe(true);
    expect(dialog.querySelector(".pui-confirm-title")?.textContent).toBe("Pay Hududgaz");
    expect(dialog.querySelector(".pui-confirm-body")?.textContent).toBe("29 322,19 so'm from card 4471");
    const [cancel, confirm] = [...dialog.querySelectorAll<HTMLButtonElement>("button")];
    expect(cancel?.textContent).toBe("Cancel");
    expect(confirm?.textContent).toBe("Pay");
    confirm?.click();
    expect(h.core.calls.at(-1)).toEqual({ method: "confirm", args: ["s1", "act_pay", "n_77ad"] });
    expect(h.messages().at(-1)).toEqual({ v: 1, t: "ui.action", surface: "s1", action_id: "act_pay", confirm: "n_77ad", source: "tap" });
    expect(document.querySelector("dialog.pui-confirm")).toBeNull();
  });

  it("does not send anything when the confirm dialog is cancelled", () => {
    const h = harness();
    h.play({ surface: "s1", ops: [{ op: "confirm", action_id: "act_1", nonce: "n1", title: "Delete card?" }] });
    const before = h.sent.length;
    h.find<HTMLButtonElement>("dialog.pui-confirm .pui-button").click();
    expect(document.querySelector("dialog.pui-confirm")).toBeNull();
    expect(h.sent.length).toBe(before);
    expect(h.core.calls.some((call) => call.method === "confirm")).toBe(false);
  });

  it("arms the confirm button after a delay and disables it on expiry", () => {
    vi.useFakeTimers();
    const h = harness({ confirmDelayMs: 400 });
    h.play({ surface: "s1", ops: [{ op: "confirm", action_id: "act_1", nonce: "n1", title: "Pay", expires_ms: 1000 }] });
    const confirm = h.find<HTMLButtonElement>("dialog.pui-confirm .pui-button[data-tone=primary]");
    expect(confirm.disabled).toBe(true);
    confirm.click();
    expect(h.core.calls.some((call) => call.method === "confirm")).toBe(false);
    vi.advanceTimersByTime(400);
    expect(confirm.disabled).toBe(false);
    vi.advanceTimersByTime(600);
    expect(confirm.disabled).toBe(true);
    expect(h.find("dialog.pui-confirm .pui-confirm-status").textContent).toContain("expired");
  });

  it("closes an open confirm dialog when its surface closes", () => {
    const h = harness();
    h.play({ surface: "s1", ops: [{ op: "confirm", action_id: "act_1", nonce: "n1", title: "Pay" }] });
    h.play({ surface: "s1", ops: [{ op: "close" }] });
    expect(document.querySelector("dialog.pui-confirm")).toBeNull();
  });

  it("answers an inline ConfirmSheet once", () => {
    const h = harness();
    h.play({
      surface: "s1",
      ops: [
        {
          op: "create",
          id: "ask",
          type: "ConfirmSheet",
          props: { title: "Save this card?", body: "You can remove it later.", confirm_label: "Save", cancel_label: "Not now", on_confirm: { action_id: "act_yes" }, on_cancel: { action_id: "act_no" } },
          a11y: { role: "dialog", label: "Save this card?" },
        },
        { op: "root", id: "ask" },
      ],
    });
    const sheet = h.find<HTMLDialogElement>('[data-pui-id="ask"]');
    expect(sheet.tagName).toBe("DIALOG");
    expect(sheet.open).toBe(true);
    const [cancel, save] = [...sheet.querySelectorAll<HTMLButtonElement>("button")];
    expect(cancel?.textContent).toBe("Not now");
    save?.click();
    save?.click();
    expect(h.messages().filter((message) => message["t"] === "ui.action")).toEqual([
      expect.objectContaining({ action_id: "act_yes" }),
    ]);
    expect(sheet.getAttribute("data-answered")).toBe("confirm");
    expect(cancel?.disabled).toBe(true);
  });
});

describe("URL safety", () => {
  it.each([
    ["https://cdn.example.com/a.png", "https://cdn.example.com/a.png"],
    ["javascript:alert(1)", null],
    ["JaVaScRiPt:alert(1)", null],
    ["java\nscript:alert(1)", null],
    ["data:image/png;base64,AAAA", null],
    ["file:///etc/passwd", null],
    ["vbscript:msgbox(1)", null],
    ["http://example.com/a.png", null],
    ["//example.com/a.png", null],
    ["/relative.png", null],
    ["https://user:pass@example.com/a.png", null],
    [" https://example.com/a.png", null],
  ])("safeUrl(%j) is %j", (raw, expected) => {
    expect(safeUrl(raw)).toBe(expected);
  });

  it("allows http only when the host opts in", () => {
    expect(safeUrl("http://example.com/a.png", true)).toBe("http://example.com/a.png");
  });

  it("never sets an unsafe image src and shows the alt text instead", () => {
    const h = harness();
    h.play({
      surface: "s1",
      ops: [
        { op: "create", id: "img", type: "Image", props: { src: "javascript:alert(1)", alt: "Store front", aspect: "wide" } },
        { op: "root", id: "img" },
      ],
    });
    expect(document.querySelector("img")).toBeNull();
    expect(h.find('[data-pui-id="img"]').textContent).toBe("Store front");
    h.play({ surface: "s1", ops: [{ op: "update", id: "img", props: { src: "https://cdn.example.com/store.jpg", alt: "Store front", aspect: "wide" } }] });
    const img = h.find<HTMLImageElement>('[data-pui-id="img"] img');
    expect(img.getAttribute("src")).toBe("https://cdn.example.com/store.jpg");
    expect(img.alt).toBe("Store front");
    expect(img.getAttribute("loading")).toBe("lazy");
    expect(img.getAttribute("decoding")).toBe("async");
    h.play({ surface: "s1", ops: [{ op: "update", id: "img", props: { src: "data:image/svg+xml,<svg onload=alert(1)>", alt: "Store front" } }] });
    expect(document.querySelector("img")).toBeNull();
  });

  it("puts hostile text through textContent only", () => {
    const h = harness();
    const hostile = '<img src=x onerror="alert(1)"><script>alert(2)</script>';
    h.play({ surface: "s1", ops: [{ op: "create", id: "t", type: "Text", props: { text: hostile } }, { op: "root", id: "t" }] });
    const node = h.find('[data-pui-id="t"]');
    expect(node.textContent).toBe(hostile);
    expect(node.children).toHaveLength(0);
  });
});
