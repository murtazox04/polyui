import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { brotliCompressSync, constants, gzipSync } from "node:zlib";

export const budgetBytes = 12 * 1024;

export function measure(file) {
  const raw = readFileSync(file);
  const brotli = brotliCompressSync(raw, {
    params: { [constants.BROTLI_PARAM_QUALITY]: constants.BROTLI_MAX_QUALITY, [constants.BROTLI_PARAM_SIZE_HINT]: raw.length },
  });
  return { raw: raw.length, gzip: gzipSync(raw, { level: 9 }).length, brotli: brotli.length };
}

export function report(dist) {
  for (const name of ["polyui.js", "polyui.css", "polyui_core_bg.wasm"]) {
    const size = measure(join(dist, name));
    console.log(`${name.padEnd(19)} raw ${size.raw} B, gzip ${size.gzip} B, brotli ${size.brotli} B`);
  }
  const js = measure(join(dist, "polyui.js"));
  console.log(`renderer budget: ${js.brotli} / ${budgetBytes} B brotli ${js.brotli < budgetBytes ? "ok" : "OVER"}`);
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  report(join(dirname(fileURLToPath(import.meta.url)), "..", "dist"));
}
