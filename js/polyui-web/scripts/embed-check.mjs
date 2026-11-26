import { existsSync, readFileSync, statSync } from "node:fs";
import { createServer } from "node:http";
import { createRequire } from "node:module";
import { delimiter, dirname, extname, join, normalize, sep } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const screenshot = join(root, "..", "..", "docs", "images", "embed-demo.png");
const types = { ".html": "text/html; charset=utf-8", ".js": "text/javascript", ".css": "text/css", ".wasm": "application/wasm", ".map": "application/json" };

function playwright() {
  const bin = (process.env.PATH ?? "").split(delimiter).find((dir) => dir.endsWith(join("node_modules", ".bin")) && existsSync(join(dir, "playwright")));
  if (!bin) throw new Error("run this through npx: npx -y -p playwright node scripts/embed-check.mjs");
  return createRequire(join(bin, "..", "playwright", "package.json"))("playwright");
}

function serve(directory, host, port) {
  const server = createServer((request, response) => {
    const path = normalize(join(directory, decodeURIComponent(new URL(request.url ?? "/", "http://x").pathname)));
    const file = path.startsWith(directory + sep) || path === directory ? (existsSync(path) && statSync(path).isDirectory() ? join(path, "index.html") : path) : "";
    if (!file || !existsSync(file)) {
      response.writeHead(404).end();
      return;
    }
    response.writeHead(200, { "content-type": types[extname(file)] ?? "application/octet-stream" }).end(readFileSync(file));
  });
  return new Promise((ready) => server.listen(port, host, () => ready(server)));
}

const servers = await Promise.all([serve(root, "localhost", 4190), serve(join(root, "embed-sandbox"), "127.0.0.1", 4191)]);
const { chromium } = playwright();
const browser = await chromium.launch({ channel: process.env.PLAYWRIGHT_CHANNEL ?? "chrome" });
const results = {};
try {
  const page = await browser.newPage({ viewport: { width: 760, height: 480 }, deviceScaleFactor: 2 });
  const problems = [];
  page.on("pageerror", (error) => problems.push(error.message));
  await page.goto("http://localhost:4190/demo/embed/");
  const outer = page.locator("iframe.pui-embed-frame");
  const app = page.frameLocator("iframe.pui-embed-frame").frameLocator("iframe");
  await app.locator("#days li").first().waitFor();
  await page.waitForFunction(() => document.querySelector("iframe.pui-embed-frame")?.style.height !== "160px");
  results.outerSandbox = await outer.getAttribute("sandbox");
  results.outerSrc = await outer.getAttribute("src");
  results.outerHasSrcdoc = (await outer.getAttribute("srcdoc")) !== null;
  results.innerSandbox = await page.frameLocator("iframe.pui-embed-frame").locator("iframe").getAttribute("sandbox");
  results.city = await app.locator("#city").textContent();
  await app.locator("#network").filter({ hasText: "Network" }).waitFor();
  results.network = await app.locator("#network").textContent();
  await app.locator("#ask").click();
  await app.locator("#status").filter({ hasText: "Sent" }).waitFor();
  results.frameHeight = await outer.evaluate((node) => node.style.height);
  results.appHeight = await app.locator("body").evaluate((node) => node.ownerDocument.documentElement.scrollHeight);
  results.sent = JSON.parse((await page.locator("#wire li").last().textContent()) ?? "null");
  results.hostCannotReachApp = await page.evaluate(() => {
    try {
      return document.querySelector("iframe.pui-embed-frame")?.contentDocument === null;
    } catch {
      return true;
    }
  });
  results.pageErrors = problems;
  await page.screenshot({ path: screenshot, fullPage: true });
  results.screenshot = screenshot;
  results.initialTheme = await app.locator("html").getAttribute("data-theme");
  await page.emulateMedia({ colorScheme: "dark" });
  await app.locator("html[data-theme='dark']").waitFor({ state: "attached" });
  results.themeAfterSystemDark = await app.locator("html").getAttribute("data-theme");
  await page.emulateMedia({ colorScheme: "light" });
  await app.locator("html[data-theme='light']").waitFor({ state: "attached" });
  results.themeAfterSystemLight = await app.locator("html").getAttribute("data-theme");
} finally {
  await browser.close();
  for (const server of servers) server.close();
}
console.log(JSON.stringify(results, null, 2));
