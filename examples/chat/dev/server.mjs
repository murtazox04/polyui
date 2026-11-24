import { readFileSync } from "node:fs";
import { createServer } from "node:http";
import { extname, join, normalize, sep } from "node:path";
import { fileURLToPath } from "node:url";
import { WebSocketServer } from "ws";

const here = fileURLToPath(new URL(".", import.meta.url));
const dist = join(here, "..", "dist");
const script = JSON.parse(readFileSync(join(here, "conversation.json"), "utf8"));
const port = Number(process.env.PORT ?? 4190);

const types = {
  ".html": "text/html; charset=utf-8",
  ".js": "text/javascript; charset=utf-8",
  ".css": "text/css; charset=utf-8",
  ".map": "application/json",
  ".wasm": "application/wasm",
  ".svg": "image/svg+xml",
};

const server = createServer((request, response) => {
  const path = new URL(request.url ?? "/", "http://localhost").pathname;
  if (path === "/config.json") {
    response.writeHead(200, { "content-type": "application/json", "cache-control": "no-store" });
    response.end(JSON.stringify({ embed_origin: null }));
    return;
  }
  const file = normalize(join(dist, path === "/" ? "index.html" : path));
  if (!file.startsWith(dist + sep)) {
    response.writeHead(403).end();
    return;
  }
  try {
    const body = readFileSync(file);
    response.writeHead(200, { "content-type": types[extname(file)] ?? "application/octet-stream", "cache-control": "no-store" });
    response.end(body);
  } catch {
    console.error(`404 ${path}`);
    response.writeHead(404).end("not found");
  }
});

const sockets = new WebSocketServer({ server, path: "/ws" });

sockets.on("connection", (socket) => {
  let greeted = false;
  let opened = false;
  const send = (message) => socket.send(JSON.stringify(message));
  const play = (messages, gap) => messages.forEach((message, index) => setTimeout(() => send(message), gap * (index + 1)));

  socket.on("message", (data) => {
    let message;
    try {
      message = JSON.parse(String(data));
    } catch {
      return;
    }
    console.log(`client ${message.t}${message.action_id ? ` ${message.action_id}` : ""}`);
    if (message.t === "hello" && !greeted) {
      greeted = true;
      send(script.welcome);
    } else if (message.t === "user.say" && !opened) {
      opened = true;
      play(script.answer, 45);
      setTimeout(() => {
        send(script.open);
        send({ v: 1, t: "agent.done" });
      }, 45 * (script.answer.length + 2));
    } else if (message.t === "user.say") {
      play(script.unknown, 60);
    } else if (message.t === "ui.action" && message.action_id === "act_pay" && message.confirm === undefined) {
      setTimeout(() => send(script.confirm), 200);
    } else if (message.t === "ui.action" && message.action_id === "act_pay") {
      play(script.paid, 120);
    } else if (message.t === "ui.action" && message.action_id === "act_month") {
      const row = message.detail?.row;
      const entry = script.months[row];
      const previous = script.months[row - 1];
      if (!entry) return;
      const change = previous ? Math.round(((entry.kwh - previous.kwh) / previous.kwh) * 100) : null;
      const text =
        change === null
          ? `${entry.month}: ${entry.kwh} kWh, the first month on the chart.`
          : `${entry.month}: ${entry.kwh} kWh, ${change >= 0 ? "up" : "down"} ${Math.abs(change)}% on ${previous.month}.`;
      play([{ v: 1, t: "agent.delta", text }, { v: 1, t: "agent.done" }], 80);
    }
  });
});

server.listen(port, () => console.log(`chat dev server: http://localhost:${port}/`));
