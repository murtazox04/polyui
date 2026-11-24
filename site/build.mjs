import { execFileSync } from "node:child_process";
import { copyFileSync, existsSync, mkdirSync, mkdtempSync, readdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import * as esbuild from "esbuild";

const site = dirname(fileURLToPath(import.meta.url));
const repo = join(site, "..");
const web = join(repo, "js", "polyui-web");
const out = join(site, "dist");
const demos = join(out, "demos");

function run(command, args, cwd = repo) {
  execFileSync(command, args, { cwd, stdio: "inherit" });
}

function ensureMermaid() {
  const target = join(site, "mermaid.min.js");
  if (existsSync(target)) return;
  const scratch = mkdtempSync(join(tmpdir(), "polyui-mermaid-"));
  writeFileSync(join(scratch, "book.toml"), '[book]\ntitle = "mermaid"\n');
  run("mdbook-mermaid", ["install", scratch], scratch);
  copyFileSync(join(scratch, "mermaid.min.js"), target);
  rmSync(scratch, { recursive: true, force: true });
}

function dropUnloadHandler() {
  const script = readdirSync(out).find((name) => /^book-[0-9a-f]+\.js$/.test(name));
  if (!script) throw new Error("mdBook did not write book-<hash>.js");
  const path = join(out, script);
  const text = readFileSync(path, "utf8");
  const handler = "window.onunload = function() { };";
  if (!text.includes(handler)) return;
  writeFileSync(path, text.replace(handler, ""));
}

function rendererDemo() {
  const target = join(demos, "renderer");
  mkdirSync(target, { recursive: true });
  const page = readFileSync(join(web, "demo", "index.html"), "utf8");
  const links = page.split('"../dist/').length - 1;
  if (links !== 2) throw new Error(`expected 2 links to ../dist/ in the renderer demo, found ${links}`);
  const icon = '</title>\n    <link rel="icon" href="./favicon.svg" type="image/svg+xml" />';
  writeFileSync(join(target, "index.html"), page.replaceAll('"../dist/', '"./').replace("</title>", icon));
  copyFileSync(join(repo, "examples", "chat", "favicon.svg"), join(target, "favicon.svg"));
  copyFileSync(join(web, "dist", "polyui.css"), join(target, "polyui.css"));
  copyFileSync(join(web, "dist", "demo.js"), join(target, "demo.js"));
}

async function dashboardDemo() {
  const source = join(site, "demos", "dashboard");
  const target = join(demos, "dashboard");
  mkdirSync(target, { recursive: true });
  const shared = { absWorkingDir: site, bundle: true, minify: true, legalComments: "none", logLevel: "warning", target: "es2022" };
  await esbuild.build({ ...shared, entryPoints: [join(source, "main.ts")], outfile: join(target, "dashboard.js"), format: "esm" });
  await esbuild.build({ ...shared, entryPoints: [join(source, "style.css")], outfile: join(target, "dashboard.css") });
  copyFileSync(join(source, "index.html"), join(target, "index.html"));
  copyFileSync(join(web, "dist", "polyui.css"), join(target, "polyui.css"));
  copyFileSync(join(web, "dist", "polyui_core_bg.wasm"), join(target, "polyui_core_bg.wasm"));
  copyFileSync(join(repo, "examples", "chat", "favicon.svg"), join(target, "favicon.svg"));
}

run(process.execPath, [join(web, "scripts", "build.mjs")], web);
ensureMermaid();
run("mdbook", ["build"], site);
dropUnloadHandler();
run(process.execPath, [join(repo, "examples", "chat", "build.mjs"), "--replay", "--out", join(demos, "chat")], join(repo, "examples", "chat"));
rendererDemo();
await dashboardDemo();
writeFileSync(join(out, ".nojekyll"), "");
console.log(`site built in ${out}`);
