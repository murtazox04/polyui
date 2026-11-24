import { readdirSync } from "node:fs";
import { dirname, join, relative, sep } from "node:path";
import { fileURLToPath } from "node:url";
import { chromium } from "playwright-core";
import { prefix, siteRoot, startServer } from "./serve.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const images = join(here, "..", "..", "docs", "images");
const port = Number(process.env.PORT ?? 4301);
const base = `http://127.0.0.1:${port}${prefix}`;

function htmlPages(directory) {
  return readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
    const path = join(directory, entry.name);
    if (entry.isDirectory()) return entry.name === "demos" ? [] : htmlPages(path);
    return entry.name.endsWith(".html") ? [relative(siteRoot, path).split(sep).join("/")] : [];
  });
}

function watch(page, label, problems) {
  page.on("console", (message) => {
    if (message.type() === "error" || message.type() === "warning") problems.push(`${label}: console ${message.type()}: ${message.text()}`);
  });
  page.on("pageerror", (error) => problems.push(`${label}: page error: ${error.message}`));
  page.on("response", (response) => {
    if (response.status() >= 400) problems.push(`${label}: ${response.status()} for ${response.url()}`);
  });
}

async function checkPage(browser, path, problems) {
  const page = await browser.newPage({ viewport: { width: 1280, height: 900 } });
  watch(page, path, problems);
  await page.goto(base + path, { waitUntil: "load" });
  const diagrams = await page.locator("pre.mermaid").count();
  if (diagrams > 0) {
    await page.waitForFunction(() => [...document.querySelectorAll("pre.mermaid")].every((block) => block.hasAttribute("data-drawn")), null, { timeout: 20000 });
    const broken = await page.evaluate(
      () => [...document.querySelectorAll("pre.mermaid")].filter((block) => !block.querySelector("svg") || /Syntax error/i.test(block.textContent ?? "")).length,
    );
    if (broken > 0) problems.push(`${path}: ${broken} of ${diagrams} diagrams did not render`);
  }
  const images = await page.evaluate(() => [...document.querySelectorAll("main img")].filter((image) => !image.complete || image.naturalWidth === 0).map((image) => image.getAttribute("src")));
  for (const source of images) problems.push(`${path}: image did not load: ${source}`);
  const title = await page.locator("main h1").first().textContent().catch(() => null);
  await page.close();
  return { path, title: title?.trim() ?? null, diagrams };
}

async function checkDarkDiagrams(browser, problems) {
  const page = await browser.newPage({ viewport: { width: 1280, height: 900 }, colorScheme: "dark" });
  watch(page, "dark concepts", problems);
  await page.goto(base + "docs/concepts.html");
  await page.waitForFunction(() => document.documentElement.classList.contains("coal"));
  await page.waitForFunction(() => [...document.querySelectorAll("pre.mermaid")].every((block) => block.hasAttribute("data-drawn") && block.querySelector("svg")));
  const background = await page.evaluate(() => getComputedStyle(document.body).backgroundColor);
  await page.click("#mdbook-theme-toggle");
  await page.click("#mdbook-theme-light");
  await page.waitForFunction(() => document.documentElement.classList.contains("light"));
  await page.waitForFunction(() => [...document.querySelectorAll("pre.mermaid")].every((block) => block.hasAttribute("data-drawn") && block.querySelector("svg")));
  const label = await page.locator("#mdbook-theme-coal").textContent();
  await page.close();
  return { darkBackground: background, themeLabel: label };
}

async function checkChat(browser, problems) {
  const page = await browser.newPage({ viewport: { width: 1180, height: 1000 } });
  watch(page, "demos/chat", problems);
  const wasm = page.waitForResponse((response) => response.url().endsWith("polyui_core_bg.wasm"));
  await page.goto(base + "demos/chat/");
  const wasmResponse = await wasm;
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
  const due = await page.locator('[data-pui-id="due"] .pui-stat-value').textContent();
  const status = await page.locator("#status").textContent();
  await page.close();
  return { wasm: `${wasmResponse.status()} ${wasmResponse.headers()["content-type"]}`, url: wasmResponse.url(), status, dueAfterPaying: due };
}

async function checkDashboard(browser, problems) {
  const page = await browser.newPage({ viewport: { width: 1180, height: 1000 } });
  watch(page, "demos/dashboard", problems);
  const wasm = page.waitForResponse((response) => response.url().endsWith("polyui_core_bg.wasm"));
  await page.goto(base + "demos/dashboard/");
  const wasmResponse = await wasm;
  await page.locator('#status[data-state="live"]').waitFor();
  await page.locator(".pui-chart svg").nth(1).waitFor();
  const result = await page.evaluate(() => ({
    charts: document.querySelectorAll(".pui-chart svg").length,
    stats: [...document.querySelectorAll(".pui-stat-value")].map((node) => node.textContent),
  }));
  await page.close();
  return { wasm: `${wasmResponse.status()} ${wasmResponse.headers()["content-type"]}`, ...result };
}

async function checkRendererDemo(browser, problems) {
  const page = await browser.newPage({ viewport: { width: 1180, height: 1000 } });
  watch(page, "demos/renderer", problems);
  await page.goto(base + "demos/renderer/");
  await page.locator("polyui-surface [data-pui-id]").first().waitFor();
  const components = await page.locator("polyui-surface [data-pui-id]").count();
  await page.close();
  return { components };
}

async function screenshotHome(browser) {
  const page = await browser.newPage({ viewport: { width: 1440, height: 900 }, deviceScaleFactor: 2, colorScheme: "light" });
  await page.goto(base);
  await page.waitForFunction(() => [...document.querySelectorAll("pre.mermaid")].every((block) => block.hasAttribute("data-drawn")));
  await page.locator("main img").first().evaluate((image) => image.complete || new Promise((done) => image.addEventListener("load", done, { once: true })));
  await page.waitForTimeout(7000);
  const path = join(images, "site-home.png");
  await page.screenshot({ path });
  await page.close();
  return path;
}

const server = await startServer(port);
const browser = await chromium.launch({ channel: process.env.CHROME_CHANNEL ?? "chrome" });
const problems = [];
const report = {};
try {
  const pages = htmlPages(siteRoot).filter((path) => path !== "toc.html").sort();
  report.pages = [];
  for (const path of pages) report.pages.push(await checkPage(browser, path, problems));
  report.darkDiagrams = await checkDarkDiagrams(browser, problems);
  report.chat = await checkChat(browser, problems);
  report.dashboard = await checkDashboard(browser, problems);
  report.renderer = await checkRendererDemo(browser, problems);
  if (!process.argv.includes("--no-screenshot")) report.screenshot = await screenshotHome(browser);
} finally {
  await browser.close();
  server.close();
}

const expected404 = (line) => line.startsWith("404.html: 404 ");
const real = problems.filter((line) => !expected404(line));
report.diagrams = report.pages.reduce((total, page) => total + page.diagrams, 0);
report.problems = real;
console.log(JSON.stringify(report, null, 2));
if (real.length > 0) process.exit(1);
