import { execFileSync } from "node:child_process";
import { copyFileSync, mkdirSync, rmSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import * as esbuild from "esbuild";
import { report } from "./size.mjs";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const dist = join(root, "dist");
const serve = process.argv.includes("--serve");

execFileSync(process.execPath, [join(root, "..", "..", "bindings", "wasm", "build.mjs")], { cwd: root, stdio: "inherit" });
rmSync(dist, { recursive: true, force: true });
mkdirSync(dist, { recursive: true });
copyFileSync(join(root, "wasm", "polyui_core_bg.wasm"), join(dist, "polyui_core_bg.wasm"));

const shared = {
  absWorkingDir: root,
  bundle: true,
  format: "esm",
  target: "es2022",
  minify: true,
  legalComments: "none",
  logLevel: "warning",
};

await esbuild.build({ ...shared, entryPoints: ["src/index.ts"], outfile: "dist/polyui.js", sourcemap: "linked" });
await esbuild.build({ ...shared, entryPoints: ["src/polyui.css"], outfile: "dist/polyui.css" });
execFileSync(join(root, "node_modules", ".bin", "tsc"), ["-p", "tsconfig.build.json"], { cwd: root, stdio: "inherit" });

const demo = { ...shared, entryPoints: ["demo/main.ts"], outfile: "dist/demo.js", minify: false };
if (serve) {
  const context = await esbuild.context(demo);
  await context.watch();
  const { port } = await context.serve({ servedir: root, port: Number(process.env.PORT ?? 4180) });
  console.log(`demo: http://localhost:${port}/demo/`);
} else {
  await esbuild.build(demo);
}

report(dist);
