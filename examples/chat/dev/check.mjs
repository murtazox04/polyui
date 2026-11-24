import { spawn } from "node:child_process";
import { mkdirSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { chromium } from "playwright-core";

const here = dirname(fileURLToPath(import.meta.url));
const images = join(here, "..", "..", "..", "docs", "images");
const port = 4191;
const base = `http://localhost:${port}/`;

const server = spawn(process.execPath, [join(here, "server.mjs")], { env: { ...process.env, PORT: String(port) }, stdio: ["ignore", "pipe", "inherit"] });
await new Promise((resolve) => server.stdout.on("data", (chunk) => String(chunk).includes("chat dev server") && resolve()));

const problems = [];
const browser = await chromium.launch({ channel: process.env.CHROME_CHANNEL ?? "chrome" });
try {
  for (const scheme of ["light", "dark"]) {
    const page = await browser.newPage({ viewport: { width: 1180, height: 1180 }, deviceScaleFactor: 2, colorScheme: scheme });
    page.on("console", (message) => {
      if (message.type() === "error" || message.type() === "warning") problems.push(`${scheme} console ${message.type()}: ${message.text()}`);
    });
    page.on("pageerror", (error) => problems.push(`${scheme} page error: ${error.message}`));
    page.on("response", (response) => {
      if (response.status() >= 400) problems.push(`${scheme} ${response.status()} for ${response.url()}`);
    });
    const wasm = page.waitForResponse((response) => response.url().endsWith("polyui_core_bg.wasm"));
    await page.goto(base);
    const wasmResponse = await wasm;
    console.log(`${scheme}: wasm ${wasmResponse.status()} ${wasmResponse.headers()["content-type"]}`);
    await page.locator('#status[data-state="live"]').waitFor();
    await page.locator("#prompt").fill("What do we owe this month?");
    await page.keyboard.press("Enter");
    await page.locator(".pui-chart svg .pui-hit").first().waitFor();
    await page.locator(".answer:not([data-streaming])").first().waitFor();
    await page.locator('.pui-hit[data-row="1"]').click();
    await page.getByText("Jul: 241 kWh, up 32% on Jun.").waitFor();
    await page.locator('[data-pui-id="pay"]').click();
    await page.locator("dialog.pui-confirm[open]").waitFor();
    await page.locator('dialog.pui-confirm .pui-button[data-tone="primary"]').click();
    await page.getByText("Paid. 184 500 so'm went to Hududiy elektr tarmoqlari.").waitFor();
    const summary = await page.evaluate(() => ({
      surfaces: document.querySelectorAll("polyui-surface").length,
      marks: document.querySelectorAll(".pui-chart svg g[aria-hidden] *").length,
      hits: document.querySelectorAll(".pui-hit").length,
      due: document.querySelector('[data-pui-id="due"] .pui-stat-value')?.textContent,
      pay: document.querySelector('[data-pui-id="pay"]')?.textContent,
      mic: document.querySelector("#mic")?.hasAttribute("disabled"),
      speaker: document.querySelector("#speaker")?.hasAttribute("disabled"),
    }));
    console.log(`${scheme}: ${JSON.stringify(summary)}`);
    mkdirSync(images, { recursive: true });
    const path = scheme === "light" ? join(images, "chat-demo.png") : join(tmpdir(), "polyui-chat-dark.png");
    await page.screenshot({ path });
    console.log(`${scheme}: saved ${path}`);
    await page.close();
  }
  const mobile = await browser.newPage({ viewport: { width: 390, height: 844 }, deviceScaleFactor: 3, isMobile: true, hasTouch: true });
  mobile.on("pageerror", (error) => problems.push(`mobile page error: ${error.message}`));
  await mobile.goto(base);
  await mobile.locator('#status[data-state="live"]').waitFor();
  await mobile.locator(".suggestion").first().click();
  await mobile.locator(".pui-chart svg").waitFor();
  await mobile.locator(".answer:not([data-streaming])").first().waitFor();
  await mobile.screenshot({ path: join(tmpdir(), "polyui-chat-mobile.png") });
  console.log(`mobile: saved ${join(tmpdir(), "polyui-chat-mobile.png")}`);
} finally {
  await browser.close();
  server.kill();
}

if (problems.length > 0) {
  console.error(problems.join("\n"));
  process.exit(1);
}
console.log("no console errors or warnings");
