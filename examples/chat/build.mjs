import { execFileSync } from "node:child_process";
import { copyFileSync, mkdirSync, rmSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import * as esbuild from "esbuild";

const root = dirname(fileURLToPath(import.meta.url));
const web = join(root, "..", "..", "js", "polyui-web");
const replay = process.argv.includes("--replay");
const outFlag = process.argv.indexOf("--out");
const dist = outFlag === -1 ? join(root, "dist") : resolve(process.argv[outFlag + 1] ?? "");

execFileSync(process.execPath, [join(root, "..", "..", "bindings", "wasm", "build.mjs")], { cwd: root, stdio: "inherit" });
rmSync(dist, { recursive: true, force: true });
mkdirSync(dist, { recursive: true });

const shared = { absWorkingDir: root, bundle: true, minify: true, legalComments: "none", logLevel: "warning", target: "es2022" };
await esbuild.build({ ...shared, entryPoints: ["main.ts"], outfile: join(dist, "chat.js"), format: "esm", sourcemap: "linked", define: { REPLAY: String(replay) } });
await esbuild.build({ ...shared, entryPoints: ["style.css"], outfile: join(dist, "chat.css") });
await esbuild.build({ ...shared, entryPoints: [join(web, "src", "polyui.css")], outfile: join(dist, "polyui.css") });
copyFileSync(join(root, "index.html"), join(dist, "index.html"));
copyFileSync(join(root, "favicon.svg"), join(dist, "favicon.svg"));
copyFileSync(join(web, "wasm", "polyui_core_bg.wasm"), join(dist, "polyui_core_bg.wasm"));
console.log(`chat page${replay ? " (replay)" : ""} built in ${dist}`);
