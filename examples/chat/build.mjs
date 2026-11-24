import { execFileSync } from "node:child_process";
import { copyFileSync, mkdirSync, rmSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import * as esbuild from "esbuild";

const root = dirname(fileURLToPath(import.meta.url));
const web = join(root, "..", "..", "js", "polyui-web");
const dist = join(root, "dist");

execFileSync(process.execPath, [join(root, "..", "..", "bindings", "wasm", "build.mjs")], { cwd: root, stdio: "inherit" });
rmSync(dist, { recursive: true, force: true });
mkdirSync(dist, { recursive: true });

const shared = { absWorkingDir: root, bundle: true, minify: true, legalComments: "none", logLevel: "warning", target: "es2022" };
await esbuild.build({ ...shared, entryPoints: ["main.ts"], outfile: "dist/chat.js", format: "esm", sourcemap: "linked" });
await esbuild.build({ ...shared, entryPoints: ["style.css"], outfile: "dist/chat.css" });
await esbuild.build({ ...shared, entryPoints: [join(web, "src", "polyui.css")], outfile: "dist/polyui.css" });
copyFileSync(join(root, "index.html"), join(dist, "index.html"));
copyFileSync(join(root, "favicon.svg"), join(dist, "favicon.svg"));
copyFileSync(join(web, "wasm", "polyui_core_bg.wasm"), join(dist, "polyui_core_bg.wasm"));
console.log(`chat page built in ${dist}`);
