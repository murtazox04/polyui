import { execFileSync } from "node:child_process";
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, statSync, writeFileSync } from "node:fs";
import { createServer } from "node:http";
import { tmpdir } from "node:os";
import { dirname, extname, join, normalize, sep } from "node:path";
import { fileURLToPath } from "node:url";
import { chromium } from "playwright-core";
import { prefix, startServer } from "./serve.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const repo = join(here, "..", "..");
const images = join(repo, "docs", "images");
const web = join(repo, "js", "polyui-web");
const scratch = mkdtempSync(join(tmpdir(), "polyui-record-"));
const sitePort = 4310;
const base = `http://127.0.0.1:${sitePort}${prefix}`;
const scale = 2;

const pointer = `
  if (window.top === window) {
    addEventListener("DOMContentLoaded", () => {
      const dot = document.createElement("div");
      dot.setAttribute("aria-hidden", "true");
      dot.style.cssText = "position:fixed;left:0;top:0;width:22px;height:22px;margin:-11px 0 0 -11px;border-radius:50%;background:rgba(20,30,45,.18);border:2px solid rgba(20,30,45,.55);pointer-events:none;z-index:2147483647;opacity:0;transition:opacity .2s,transform .12s;";
      document.body.append(dot);
      window.recordingPointer = {
        at(x, y) { dot.style.opacity = "1"; dot.style.left = x + "px"; dot.style.top = y + "px"; },
        press() { dot.style.transform = "scale(.7)"; setTimeout(() => { dot.style.transform = "scale(1)"; }, 160); },
      };
    });
  }
`;

function serveDirectory(directory, host, port) {
  const types = { ".html": "text/html; charset=utf-8", ".js": "text/javascript", ".css": "text/css", ".wasm": "application/wasm", ".map": "application/json" };
  const server = createServer((request, response) => {
    const path = normalize(join(directory, decodeURIComponent(new URL(request.url ?? "/", "http://x").pathname)));
    const inside = path === directory || path.startsWith(directory + sep);
    const file = inside && existsSync(path) && statSync(path).isDirectory() ? join(path, "index.html") : path;
    if (!inside || !existsSync(file)) {
      response.writeHead(404).end();
      return;
    }
    response.writeHead(200, { "content-type": types[extname(file)] ?? "application/octet-stream" }).end(readFileSync(file));
  });
  return new Promise((ready) => server.listen(port, host, () => ready(server)));
}

async function record(browser, name, viewport, script, options = {}) {
  const directory = join(scratch, name);
  mkdirSync(directory, { recursive: true });
  const context = await browser.newContext({ viewport, deviceScaleFactor: scale, colorScheme: options.colorScheme ?? "light" });
  await context.addInitScript(pointer);
  const page = await context.newPage();
  pointerAt = { x: viewport.width * 0.55, y: viewport.height * 0.35 };
  const session = await context.newCDPSession(page);
  const frames = [];
  session.on("Page.screencastFrame", (frame) => {
    const file = join(directory, `frame-${String(frames.length).padStart(5, "0")}.png`);
    writeFileSync(file, Buffer.from(frame.data, "base64"));
    frames.push({ file, time: frame.metadata.timestamp });
    void session.send("Page.screencastFrameAck", { sessionId: frame.sessionId }).catch(() => undefined);
  });
  const begin = () =>
    session.send("Page.startScreencast", { format: "png", maxWidth: viewport.width * scale, maxHeight: viewport.height * scale, everyNthFrame: 1 });
  const finish = async (hold) => {
    await page.waitForTimeout(hold);
    await session.send("Page.stopScreencast");
  };
  await script(page, begin, finish);
  await context.close();
  if (frames.length < 2) throw new Error(`${name}: only ${frames.length} frames were captured`);
  const lines = ["ffconcat version 1.0"];
  frames.forEach((frame, index) => {
    const next = frames[index + 1];
    lines.push(`file '${frame.file}'`, `duration ${(next ? next.time - frame.time : 1.2).toFixed(3)}`);
  });
  lines.push(`file '${frames.at(-1).file}'`);
  const list = join(directory, "frames.txt");
  writeFileSync(list, lines.join("\n"));
  return list;
}

function toGif(list, target, { width, fps = 12, colors = 160 }) {
  const filters = `fps=${fps},scale=${width}:-1:flags=lanczos,split[a][b];[a]palettegen=max_colors=${colors}:stats_mode=full[p];[b][p]paletteuse=dither=none:diff_mode=rectangle`;
  execFileSync("ffmpeg", ["-y", "-loglevel", "error", "-f", "concat", "-safe", "0", "-i", list, "-vf", filters, "-loop", "0", target]);
  return statSync(target).size;
}

let pointerAt = { x: 380, y: 300 };

async function move(page, locator, steps = 18) {
  const box = await locator.boundingBox();
  if (!box) throw new Error("nothing to point at");
  const target = { x: box.x + box.width / 2, y: box.y + box.height / 2 };
  for (let step = 1; step <= steps; step += 1) {
    const progress = 1 - (1 - step / steps) ** 3;
    const x = pointerAt.x + (target.x - pointerAt.x) * progress;
    const y = pointerAt.y + (target.y - pointerAt.y) * progress;
    await page.mouse.move(x, y);
    await page.evaluate(([left, top]) => window.recordingPointer?.at(left, top), [x, y]);
    await page.waitForTimeout(16);
  }
  pointerAt = target;
  return box;
}

async function press(page, locator) {
  await page.evaluate(() => window.recordingPointer?.press());
  await locator.click();
}

async function chatAnswer(page, begin, finish) {
  await page.goto(`${base}demos/chat/`);
  await page.locator('#status[data-state="live"]').waitFor();
  await page.waitForTimeout(500);
  await begin();
  await page.waitForTimeout(700);
  await move(page, page.locator("#prompt"));
  await press(page, page.locator("#prompt"));
  await page.keyboard.type("What do we owe this month?", { delay: 45 });
  await page.waitForTimeout(250);
  await page.keyboard.press("Enter");
  await page.locator(".pui-chart svg .pui-hit").first().waitFor();
  await page.locator(".answer:not([data-streaming])").first().waitFor();
  await finish(2200);
}

async function chatPay(page, begin, finish) {
  await page.goto(`${base}demos/chat/`);
  await page.locator('#status[data-state="live"]').waitFor();
  await page.locator("#prompt").fill("What do we owe this month?");
  await page.keyboard.press("Enter");
  await page.locator(".pui-chart svg .pui-hit").first().waitFor();
  await page.locator(".answer:not([data-streaming])").first().waitFor();
  await page.waitForTimeout(800);
  await begin();
  await move(page, page.locator('.pui-hit[data-row="1"]'), 24);
  await page.waitForTimeout(300);
  await press(page, page.locator('.pui-hit[data-row="1"]'));
  await page.getByText("Jul: 241 kWh, up 32% on Jun.").waitFor();
  await page.waitForTimeout(1400);
  await move(page, page.locator('[data-pui-id="pay"]'), 24);
  await page.waitForTimeout(250);
  await press(page, page.locator('[data-pui-id="pay"]'));
  await page.locator("dialog.pui-confirm[open]").waitFor();
  await page.waitForTimeout(900);
  const confirm = page.locator('dialog.pui-confirm .pui-button[data-tone="primary"]');
  await move(page, confirm, 16);
  await page.waitForTimeout(300);
  await press(page, confirm);
  await page.getByText("Paid. 184 500 so'm went to Hududiy elektr tarmoqlari.").waitFor();
  await finish(2200);
}

async function embed(page, begin, finish) {
  await page.goto("http://localhost:4190/demo/embed/");
  await begin();
  const app = page.frameLocator("iframe.pui-embed-frame").frameLocator("iframe");
  await app.locator("#days li").first().waitFor();
  await page.waitForFunction(() => document.querySelector("iframe.pui-embed-frame")?.style.height !== "160px");
  await page.waitForTimeout(1200);
  const ask = app.locator("#ask");
  await move(page, ask, 24);
  await page.waitForTimeout(300);
  await press(page, ask);
  await app.locator("#status").filter({ hasText: "Sent" }).waitFor();
  await page.locator("#wire li").first().waitFor();
  await finish(2400);
}

async function dashboardShot(browser) {
  const page = await browser.newPage({ viewport: { width: 1100, height: 760 }, deviceScaleFactor: scale, colorScheme: "light" });
  await page.goto(`${base}demos/dashboard/`);
  await page.locator('#status[data-state="live"]').waitFor();
  await page.locator(".pui-chart svg").nth(1).waitFor();
  const path = join(images, "dashboard.png");
  await page.locator("#stage").screenshot({ path });
  await page.close();
  return statSync(path).size;
}

const site = await startServer(sitePort);
const embedServers = await Promise.all([serveDirectory(web, "localhost", 4190), serveDirectory(join(web, "embed-sandbox"), "127.0.0.1", 4191)]);
const browser = await chromium.launch({ channel: process.env.CHROME_CHANNEL ?? "chrome" });
const sizes = {};
try {
  const answer = await record(browser, "answer", { width: 760, height: 820 }, chatAnswer);
  sizes["chat-answer.gif"] = toGif(answer, join(images, "chat-answer.gif"), { width: 1140 });
  const pay = await record(browser, "pay", { width: 760, height: 820 }, chatPay);
  sizes["chat-pay.gif"] = toGif(pay, join(images, "chat-pay.gif"), { width: 1140 });
  const frame = await record(browser, "embed", { width: 700, height: 640 }, embed);
  sizes["embed.gif"] = toGif(frame, join(images, "embed.gif"), { width: 1050 });
  sizes["dashboard.png"] = await dashboardShot(browser);
} finally {
  await browser.close();
  site.close();
  for (const server of embedServers) server.close();
  rmSync(scratch, { recursive: true, force: true });
}
console.log(JSON.stringify(sizes, null, 2));
