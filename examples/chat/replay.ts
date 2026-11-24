import conversation from "./dev/conversation.json";

type Message = { readonly [key: string]: unknown };

const openState = 1;
const closedState = 3;

function parse(text: string): Message | null {
  try {
    const value: unknown = JSON.parse(text);
    return typeof value === "object" && value !== null && !Array.isArray(value) ? (value as Message) : null;
  } catch {
    return null;
  }
}

function monthText(row: number): string | null {
  const entry = conversation.months[row];
  if (!entry) return null;
  const previous = conversation.months[row - 1];
  if (!previous) return `${entry.month}: ${entry.kwh} kWh, the first month on the chart.`;
  const change = Math.round(((entry.kwh - previous.kwh) / previous.kwh) * 100);
  return `${entry.month}: ${entry.kwh} kWh, ${change >= 0 ? "up" : "down"} ${Math.abs(change)}% on ${previous.month}.`;
}

export class ReplaySocket extends EventTarget {
  readyState = 0;
  #greeted = false;
  #opened = false;
  #timers = new Set<ReturnType<typeof setTimeout>>();

  constructor() {
    super();
    this.#later(0, () => {
      this.readyState = openState;
      this.dispatchEvent(new Event("open"));
    });
  }

  send(data: string): void {
    if (this.readyState !== openState) return;
    const message = parse(data);
    if (!message) return;
    const type = message["t"];
    const action = message["action_id"];
    if (type === "hello" && !this.#greeted) {
      this.#greeted = true;
      this.#later(0, () => this.#deliver(conversation.welcome));
    } else if (type === "user.say" && !this.#opened) {
      this.#opened = true;
      this.#play(conversation.answer, 45);
      this.#later(45 * (conversation.answer.length + 2), () => {
        this.#deliver(conversation.open);
        this.#deliver({ v: 1, t: "agent.done" });
      });
    } else if (type === "user.say") {
      this.#play(conversation.unknown, 60);
    } else if (type === "ui.action" && action === "act_pay" && message["confirm"] === undefined) {
      this.#later(200, () => this.#deliver(conversation.confirm));
    } else if (type === "ui.action" && action === "act_pay") {
      this.#play(conversation.paid, 120);
    } else if (type === "ui.action" && action === "act_month") {
      const detail = message["detail"];
      const row = typeof detail === "object" && detail !== null ? Reflect.get(detail, "row") : undefined;
      const text = typeof row === "number" ? monthText(row) : null;
      if (text !== null) this.#play([{ v: 1, t: "agent.delta", text }, { v: 1, t: "agent.done" }], 80);
    }
  }

  close(): void {
    for (const timer of this.#timers) clearTimeout(timer);
    this.#timers.clear();
    this.readyState = closedState;
  }

  #deliver(message: unknown): void {
    if (this.readyState !== openState) return;
    this.dispatchEvent(new MessageEvent("message", { data: JSON.stringify(message) }));
  }

  #play(messages: readonly unknown[], gap: number): void {
    messages.forEach((message, index) => this.#later(gap * (index + 1), () => this.#deliver(message)));
  }

  #later(ms: number, run: () => void): void {
    const timer = setTimeout(() => {
      this.#timers.delete(timer);
      run();
    }, ms);
    this.#timers.add(timer);
  }
}
