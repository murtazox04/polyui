import { mount } from "../src/index";
import type { A11y, Json, Props, RenderBatch, RenderOp } from "../src/types";
import { FakeCore } from "../test/fake-core";
import { billsScene, months } from "./scene";

const surfaceId = "s1";
const offers = {
  full: { label: "Pay in full", detail: "41 722 so'm today", now: "41 722" },
  cashback: { label: "Pay with Humo, 2% back", detail: "41 722 so'm, 834 so'm returns to you", now: "41 722" },
  split: { label: "Split in two", detail: "20 861 so'm now, the rest on 20 December", now: "20 861" },
} as const;

type OfferId = keyof typeof offers;

function isOffer(value: unknown): value is OfferId {
  return typeof value === "string" && value in offers;
}

function required<T extends Element>(selector: string): T {
  const found = document.querySelector<T>(selector);
  if (!found) throw new Error(`demo markup is missing ${selector}`);
  return found;
}

const stage = required<HTMLElement>("#stage");
const wire = required<HTMLOListElement>("#wire");
const spoken = required<HTMLParagraphElement>("#spoken");
const themeButton = required<HTMLButtonElement>("#theme");
const replayButton = required<HTMLButtonElement>("#replay");
const readButton = required<HTMLButtonElement>("#read");

const core = new FakeCore();
let offer: OfferId = "full";
let paid = false;
const timers: ReturnType<typeof setTimeout>[] = [];

const renderer = mount(stage, {
  core,
  send: (message) => {
    logWire("send", message);
    respond(JSON.parse(message) as Json);
  },
});

function later(ms: number, run: () => void): void {
  timers.push(setTimeout(run, ms));
}

function logWire(direction: "send" | "recv", message: string): void {
  const parsed = JSON.parse(message) as { [key: string]: Json };
  const item = document.createElement("li");
  item.className = `wire-item wire-${direction}`;
  const head = document.createElement("span");
  head.className = "wire-head";
  head.textContent = `${direction === "send" ? "client" : "server"}  ${String(parsed["t"] ?? "batch")}`;
  const body = document.createElement("code");
  body.className = "wire-body";
  const { v, t, ...rest } = parsed;
  body.textContent = JSON.stringify(rest);
  item.append(head, body);
  wire.prepend(item);
  while (wire.childElementCount > 14) wire.lastElementChild?.remove();
}

function deliver(message: { [key: string]: Json } | RenderBatch): void {
  const text = JSON.stringify(message);
  logWire("recv", text);
  renderer.receive(text);
}

function create(id: string, type: string, props: Props, a11y?: A11y, value?: Json): RenderOp {
  return { op: "create", id, type, props, ...(a11y ? { a11y } : {}), ...(value !== undefined ? { value } : {}) };
}

function payButton(label: string, disabled: boolean): RenderOp {
  return {
    op: "update",
    id: "pay",
    props: { label, tone: disabled ? "secondary" : "primary", on_press: { action_id: "act_pay", confirm: true }, disabled },
    a11y: { role: "button", label },
  };
}

function offerProps(): Props {
  return {
    label: "Choose an offer",
    options: (Object.keys(offers) as OfferId[]).map((id) => ({ id, label: offers[id].label, detail: offers[id].detail })),
    multiple: false,
    value: "/offer",
    on_change: { action_id: "act_offer", confirm: false },
  };
}

function openBatch(): RenderBatch {
  return {
    surface: surfaceId,
    ops: [
      create("card", "Card", { title: "November utilities", subtitle: "Account 4471, Chilonzor, Tashkent", tone: "plain" }, { role: "group", label: "November utilities" }),
      create("stats", "Row", { gap: "large", align: "start" }, { role: "group" }),
      create("due", "Stat", { label: "Due now", value: "41 722", unit: "so'm", tone: "negative", hint: "Gas and power, due 5 December" }, { role: "status", label: "Due now: 41 722 so'm" }),
      create("trend", "Stat", { label: "Versus October", value: "+18", unit: "%", tone: "warning", hint: "Heating season started" }, { role: "status", label: "Versus October: up 18 percent" }),
      create("chart", "chart:Chart", { title: "Last six months, thousand so'm", kind: "bar", x: "month", y: "amount", rows: months.map((entry) => ({ ...entry })), on_select: { action_id: "act_month", confirm: false }, scene: billsScene() as unknown as Json }, { role: "figure", label: "Last six months" }),
      create("note", "Text", { text: "Tap a month to see what changed.", style: "muted" }),
      create("rule", "Divider", {}),
      create("offer", "ChoiceGroup", offerProps(), { role: "radiogroup", label: "Choose an offer" }, "full"),
      create("phone", "Input", { label: "Phone for the receipt", value: "/form/phone", input: "phone", placeholder: "+998 90 123 45 67", max_length: 17, required: false }, { role: "textbox", label: "Phone for the receipt" }, ""),
      create("footer", "Row", { gap: "medium", align: "between" }, { role: "group" }),
      create("card_badge", "Badge", { text: "Humo card 4471", tone: "neutral" }, { role: "status", label: "Humo card 4471" }),
      create("pay", "Button", { label: "Pay 41 722 so'm", tone: "primary", on_press: { action_id: "act_pay", confirm: true }, disabled: false }, { role: "button", label: "Pay 41 722 so'm" }),
      { op: "children", id: "stats", children: ["due", "trend"] },
      { op: "children", id: "footer", children: ["card_badge", "pay"] },
      { op: "children", id: "card", children: ["stats", "chart", "note", "rule", "offer", "phone", "footer"] },
      { op: "root", id: "card" },
    ],
    fallback: "November utilities: due now 41 722 so'm. [Pay]",
  };
}

function respond(message: Json): void {
  if (typeof message !== "object" || message === null || Array.isArray(message) || message["t"] !== "ui.action") return;
  const action = message["action_id"];
  const values = message["values"];
  const fields = typeof values === "object" && values !== null && !Array.isArray(values) ? values : {};
  if (action === "act_offer") {
    const picked = fields["/offer"];
    if (!isOffer(picked)) return;
    offer = picked;
    later(220, () => deliver({ surface: surfaceId, ops: [payButton(`Pay ${offers[offer].now} so'm`, false)] }));
  } else if (action === "act_month") {
    const row = fields["row"];
    const entry = typeof row === "number" ? months[row] : undefined;
    const previous = typeof row === "number" ? months[row - 1] : undefined;
    if (!entry) return;
    const change = previous ? Math.round(((entry.amount - previous.amount) / previous.amount) * 100) : 0;
    const direction = change >= 0 ? `up ${change}%` : `down ${Math.abs(change)}%`;
    const text = previous ? `${entry.month}: ${entry.amount.toFixed(1)} thousand so'm, ${direction} on ${previous.month}.` : `${entry.month}: ${entry.amount.toFixed(1)} thousand so'm.`;
    later(160, () => deliver({ surface: surfaceId, ops: [{ op: "update", id: "note", props: { text, style: "body" } }] }));
  } else if (action === "act_pay" && message["confirm"] === undefined && !paid) {
    const amount = offers[offer].now;
    later(260, () =>
      deliver({
        v: 1,
        t: "ui.confirm",
        surface: surfaceId,
        action_id: "act_pay",
        nonce: `n_${Math.random().toString(36).slice(2, 6)}`,
        title: offer === "split" ? "Pay the first half?" : "Pay gas and power?",
        body: `${amount} so'm from Humo card 4471 to Hududgaz and Hududiy elektr tarmoqlari.`,
        confirm_label: `Pay ${amount} so'm`,
        expires_ms: 30000,
      }),
    );
  } else if (action === "act_pay" && typeof message["confirm"] === "string") {
    paid = true;
    const remaining = offer === "split" ? "20 861" : "0";
    later(520, () =>
      deliver({
        surface: surfaceId,
        ops: [
          { op: "update", id: "card", props: { title: "November utilities", subtitle: "Paid just now from card 4471", tone: "success" }, a11y: { role: "group", label: "November utilities, paid" } },
          { op: "update", id: "due", props: { label: "Due now", value: remaining, unit: "so'm", tone: remaining === "0" ? "positive" : "warning", hint: remaining === "0" ? "All settled for November" : "Second half due 20 December" }, a11y: { role: "status", label: `Due now: ${remaining} so'm` } },
          { op: "update", id: "card_badge", props: { text: "Payment sent", tone: "success" }, a11y: { role: "status", label: "Payment sent" } },
          payButton("Paid", true),
          { op: "remove", id: "offer" },
          { op: "remove", id: "rule" },
          { op: "children", id: "card", children: ["stats", "chart", "note", "phone", "footer"] },
        ],
      }),
    );
  }
}

function play(): void {
  for (const id of timers.splice(0)) clearTimeout(id);
  offer = "full";
  paid = false;
  spoken.textContent = "";
  renderer.receive(JSON.stringify({ surface: surfaceId, ops: [{ op: "close" }] }));
  later(380, () => deliver(openBatch()));
}

const themes = ["light dark", "light", "dark"] as const;
const themeNames = { "light dark": "System", light: "Light", dark: "Dark" } as const;
let themeIndex = 0;

themeButton.addEventListener("click", () => {
  themeIndex = (themeIndex + 1) % themes.length;
  const scheme = themes[themeIndex] ?? "light dark";
  document.documentElement.style.setProperty("--pui-scheme", scheme);
  themeButton.textContent = `Theme: ${themeNames[scheme]}`;
});

replayButton.addEventListener("click", play);

readButton.addEventListener("click", () => {
  const utterances = renderer.say(surfaceId);
  utterances.forEach((utterance, index) => {
    later(index * 1100, () => {
      renderer.highlight(surfaceId, utterance.id);
      spoken.textContent = utterance.text;
    });
  });
  later(utterances.length * 1100, () => {
    renderer.highlight(surfaceId, null);
    spoken.textContent = "";
  });
});

play();
