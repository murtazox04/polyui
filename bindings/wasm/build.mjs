import { execFileSync } from "node:child_process";
import { mkdirSync, readFileSync, rmSync } from "node:fs";
import { homedir } from "node:os";
import { delimiter, dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { brotliCompressSync, constants, gzipSync } from "node:zlib";

const here = dirname(fileURLToPath(import.meta.url));
const workspace = join(here, "..", "..");
const out = join(workspace, "js", "polyui-web", "wasm");
const built = join(workspace, "target", "wasm32-unknown-unknown", "wasm", "polyui_wasm.wasm");
const wasm = join(out, "polyui_core_bg.wasm");
const adr = join(workspace, "docs", "adr", "0001-stack.md");
const budget = readFileSync(adr, "utf8").match(/budget is (\d[\d ]*) bytes brotli/);
if (!budget) throw new Error(`no "budget is N bytes brotli" sentence in ${adr}`);
const budgetBytes = Number(budget[1].replaceAll(" ", ""));

const path = [process.env.PATH, join(homedir(), ".cargo", "bin")].filter(Boolean).join(delimiter);

function run(command, args, env = {}) {
  execFileSync(command, args, { cwd: workspace, stdio: "inherit", env: { ...process.env, PATH: path, ...env } });
}

function sizes(file) {
  const raw = readFileSync(file);
  const brotli = brotliCompressSync(raw, {
    params: { [constants.BROTLI_PARAM_QUALITY]: constants.BROTLI_MAX_QUALITY, [constants.BROTLI_PARAM_SIZE_HINT]: raw.length },
  });
  return { raw: raw.length, gzip: gzipSync(raw, { level: 9 }).length, brotli: brotli.length };
}

function line(label, size) {
  return `${label.padEnd(22)} raw ${String(size.raw).padStart(7)} B, gzip ${String(size.gzip).padStart(6)} B, brotli ${String(size.brotli).padStart(6)} B`;
}

run("cargo", ["build", "--profile", "wasm", "--target", "wasm32-unknown-unknown", "-p", "polyui-wasm"], { CARGO_PROFILE_WASM_STRIP: "false" });
rmSync(out, { recursive: true, force: true });
mkdirSync(out, { recursive: true });
run("wasm-bindgen", ["--target", "web", "--out-dir", out, "--out-name", "polyui_core", built]);
run("wasm-opt", ["-O1", "--strip-debug", "--strip-producers", "--enable-bulk-memory", "--enable-nontrapping-float-to-int", "--enable-sign-ext", "--enable-mutable-globals", "--enable-reference-types", "--enable-multivalue", wasm, "-o", wasm]);
const after = sizes(wasm);
const glue = sizes(join(out, "polyui_core.js"));

console.log(line("polyui_core_bg.wasm", after));
console.log(line("polyui_core.js", glue));
console.log(`wasm budget: ${after.brotli} / ${budgetBytes} B brotli ${after.brotli <= budgetBytes ? "ok" : "OVER"}`);
