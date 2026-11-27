import { execFileSync } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";
import { createRequire } from "node:module";
import { delimiter, dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..", "..", "..", "..");
const nonce = "checkNonce0123456789abcdef";
const uri = "ui://weather/forecast";

function playwright() {
  const bin = (process.env.PATH ?? "").split(delimiter).find((dir) => dir.endsWith(join("node_modules", ".bin")) && existsSync(join(dir, "playwright")));
  if (!bin) throw new Error("run this through npx: npx -y -p playwright node bindings/ffi/tests/embed/check.mjs");
  return createRequire(join(bin, "..", "playwright", "package.json"))("playwright");
}

const props = {
  resource: uri,
  title: "Five day forecast for Tashkent",
  height: "small",
  on_message: { action_id: "act_ask", confirm: false },
  app: {
    resource: { uri, mimeType: "text/html;profile=mcp-app", text: readFileSync(join(root, "js/polyui-web/demo/embed/forecast-app.html"), "utf8"), _meta: { ui: { csp: {} } } },
    tool_input: { city: "Tashkent" },
    tool_result: { content: [{ type: "text", text: "Mild days." }], structuredContent: { days: [{ name: "Tue", high: 14 }] } },
  },
};

const build = (host) => execFileSync("cargo", ["run", "-q", "-p", "polyui-ffi", "--example", "embed_page", "--", nonce, host], { cwd: root, input: JSON.stringify(props), maxBuffer: 1 << 24 }).toString();
const html = build("{}");
const withAppCalls = build(JSON.stringify({ app_calls: true }));

const { chromium } = playwright();
const browser = await chromium.launch({ channel: process.env.PLAYWRIGHT_CHANNEL ?? "chrome" });
const events = [];
const failures = [];
const expect = (condition, label) => {
  if (!condition) failures.push(label);
};

try {
  const page = await browser.newPage();
  const problems = [];
  page.on("pageerror", (error) => problems.push(error.message));
  await page.exposeBinding("polyuiNative", (source, text) => events.push({ main: source.frame === page.mainFrame(), ...JSON.parse(text) }));
  await page.addInitScript(() => {
    window.polyui = { postMessage: (text) => window.polyuiNative(text) };
  });
  await page.route("https://embed.polyui.invalid/", (route) => route.fulfill({ status: 200, contentType: "text/html; charset=utf-8", body: html }));
  await page.goto("https://embed.polyui.invalid/");

  const app = page.frameLocator("iframe");
  await app.locator("#days li").first().waitFor();
  await app.locator("#network").filter({ hasText: "blocked" }).waitFor();
  const inner = page.frames().find((frame) => frame !== page.mainFrame());

  expect((await page.locator("iframe").getAttribute("sandbox")) === "allow-scripts", "inner frame has sandbox=allow-scripts only");
  expect((await app.locator("#city").textContent()) === "Forecast for Tashkent", "tool input reaches the app");
  expect((await app.locator("#network").textContent()).includes("connect-src"), "connect-src blocks the app's fetch");
  expect(events.some((event) => event.main && event.nonce === nonce && event.kind === "ready"), "ready reaches native with the nonce");
  expect(events.some((event) => event.kind === "height" && event.value >= 80 && event.value <= 320), "height is clamped to the small bounds");

  await app.locator("#ask").click();
  await app.locator("#status").filter({ hasText: "Sent" }).waitFor();
  const sent = events.find((event) => event.kind === "message");
  expect(sent && sent.main && sent.nonce === nonce && sent.text === "Also show the forecast for Samarkand.", "ui/message reaches native as text");

  await app.locator("#ask").click();
  await app.locator("#status").filter({ hasText: "Too many" }).waitFor();
  expect(events.filter((event) => event.kind === "message").length === 1, "a second message within a second is refused");

  const answer = inner.evaluate(() => new Promise((resolve) => {
    window.addEventListener("message", (event) => {
      if (event.data && event.data.id === 77) resolve(event.data);
    });
    window.parent.postMessage({ jsonrpc: "2.0", id: 77, method: "ui/open-link", params: { url: "https://example.com/a?b=1" } }, "*");
  }));
  await new Promise((resolve) => setTimeout(resolve, 200));
  const link = events.find((event) => event.kind === "open-link");
  expect(link && link.url === "https://example.com/a?b=1" && link.nonce === nonce, "open-link reaches native");
  expect(await page.evaluate(() => window.polyuiHost.answerLink(false)), "answerLink settles the pending link");
  const denied = await answer;
  expect(denied.error && denied.error.message === "Link opening denied by user", "a refused link is an error for the app");

  const bad = await inner.evaluate(() => new Promise((resolve) => {
    window.addEventListener("message", (event) => {
      if (event.data && event.data.id === 78) resolve(event.data);
    });
    window.parent.postMessage({ jsonrpc: "2.0", id: 78, method: "ui/open-link", params: { url: "http://example.com/" } }, "*");
  }));
  expect(bad.error && bad.error.message === "Invalid URL", "plain http links are refused");

  await inner.evaluate(() => window.polyui && window.polyui.postMessage(JSON.stringify({ kind: "message", text: "spoofed" })));
  await new Promise((resolve) => setTimeout(resolve, 200));
  const spoofed = events.find((event) => event.text === "spoofed");
  expect(!spoofed || (spoofed.nonce !== nonce && !spoofed.main), "the app cannot forge a native event with the nonce");

  const reach = await inner.evaluate(() => {
    try {
      return typeof window.parent.polyuiHost;
    } catch {
      return "blocked";
    }
  });
  expect(reach === "blocked", "the app cannot reach polyuiHost");

  const appTheme = () => app.locator("html").getAttribute("data-theme");
  expect((await appTheme()) === "light", "the app starts with the system theme");
  await page.emulateMedia({ colorScheme: "dark" });
  await app.locator("html[data-theme='dark']").waitFor({ state: "attached", timeout: 5000 }).catch(() => undefined);
  expect((await appTheme()) === "dark", "a system theme change reaches the app");
  expect(await page.evaluate(() => window.polyuiHost.setTheme("light")), "setTheme accepts light");
  await app.locator("html[data-theme='light']").waitFor({ state: "attached", timeout: 5000 }).catch(() => undefined);
  expect((await appTheme()) === "light", "the native theme wins over the system theme");
  expect(await page.evaluate(() => window.polyuiHost.setTheme("sepia") === false), "setTheme refuses other names");

  expect(await page.evaluate(() => window.polyuiHost.update({ resource: "ui://weather/forecast" }) === false), "update with a different app asks for a reload");

  const ask = (frame, id, method, params) => frame.evaluate(([id, method, params]) => new Promise((resolve) => {
    window.addEventListener("message", (event) => {
      if (event.data && event.data.id === id) resolve(event.data);
    });
    window.parent.postMessage({ jsonrpc: "2.0", id, method, params }, "*");
  }), [id, method, params]);
  const refused = await ask(inner, 80, "tools/call", { name: "forecast", arguments: { city: "Samarkand" } });
  expect(refused.error && refused.error.code === -32601, "tools/call is refused when the host did not turn app calls on");
  expect(!events.some((event) => event.kind === "app-call"), "no app call reaches native without app calls");
  expect(problems.length === 0, `no page errors (${problems.join("; ")})`);

  const second = await browser.newPage();
  const secondEvents = [];
  await second.exposeBinding("polyuiNative", (source, text) => secondEvents.push({ main: source.frame === second.mainFrame(), ...JSON.parse(text) }));
  await second.addInitScript(() => {
    window.polyui = { postMessage: (text) => window.polyuiNative(text) };
  });
  await second.route("https://embed.polyui.invalid/", (route) => route.fulfill({ status: 200, contentType: "text/html; charset=utf-8", body: withAppCalls }));
  await second.goto("https://embed.polyui.invalid/");
  await second.frameLocator("iframe").locator("#days li").first().waitFor();
  const secondInner = second.frames().find((frame) => frame !== second.mainFrame());
  const capabilities = await ask(secondInner, 90, "ui/initialize", { appInfo: { name: "check", version: "1" }, appCapabilities: {}, protocolVersion: "2026-01-26" });
  expect(capabilities.result && capabilities.result.hostCapabilities.serverTools && capabilities.result.hostCapabilities.serverResources, "the app is told it may call server tools");
  const quoted = ask(secondInner, 91, "tools/call", { name: "forecast", arguments: { city: "Samarkand" } });
  await new Promise((resolve) => setTimeout(resolve, 200));
  const call = secondEvents.find((event) => event.kind === "app-call");
  expect(call && call.main && call.nonce === nonce && call.method === "tools/call" && call.params.arguments.city === "Samarkand", "tools/call reaches native as an app call");
  expect(call && /^[0-9a-f]{12}-1$/.test(call.id), "the call id is the page's own, not the app's");
  expect(await second.evaluate(() => window.polyuiHost.answerApp("unknown", { result: {} }) === false), "an unknown call id is ignored");
  expect(await second.evaluate((id) => window.polyuiHost.answerApp(id, { result: { content: [{ type: "text", text: "Rain" }] } }), call && call.id), "answerApp settles the call");
  const answered = await quoted;
  expect(answered.result && answered.result.content[0].text === "Rain", "the result reaches the app");
  expect(await second.evaluate((id) => window.polyuiHost.answerApp(id, { result: {} }) === false, call && call.id), "a call is answered once");
  const failing = ask(secondInner, 92, "resources/read", { uri });
  await new Promise((resolve) => setTimeout(resolve, 200));
  const read = secondEvents.filter((event) => event.kind === "app-call").at(-1);
  await second.evaluate((id) => window.polyuiHost.answerApp(id, { error: { code: -32000, message: "unknown resource" } }), read.id);
  const failed = await failing;
  expect(failed.error && failed.error.code === -32000 && failed.error.message === "unknown resource", "an error answer reaches the app");
  await second.close();
} finally {
  await browser.close();
}

for (const failure of failures) console.log(`FAIL ${failure}`);
console.log(failures.length === 0 ? "embed page check: all checks passed" : `${failures.length} checks failed`);
process.exit(failures.length === 0 ? 0 : 1);
