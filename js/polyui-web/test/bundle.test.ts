import { readFileSync, readdirSync, statSync } from "node:fs";
import { join } from "node:path";
import { brotliCompressSync, constants } from "node:zlib";
import { describe, expect, it } from "vitest";

const root = join(import.meta.dirname, "..");
const bundle = readFileSync(join(root, "dist", "polyui.js"), "utf8");
const budget = 12 * 1024;

const forbidden: [string, RegExp][] = [
  ["innerHTML", /innerHTML/],
  ["outerHTML", /outerHTML/],
  ["insertAdjacentHTML", /insertAdjacentHTML/],
  ["document.write", /document\.write/],
  ["eval", /\beval\s*\(/],
  ["new Function", /new\s+Function\b/],
  ["Function constructor", /\bFunction\s*\(/],
  ["setTimeout with a string", /setTimeout\(\s*["'`]/],
  ["string event handler attribute", /setAttribute\(\s*["'`]on/i],
  ["inline event handler property", /\.on[a-z]+\s*=(?!=)/],
  ["srcdoc", /srcdoc/i],
  ["createContextualFragment", /createContextualFragment/],
  ["DOMParser", /DOMParser/],
  ["style attribute", /setAttribute\(\s*["'`]style/],
  ["javascript url", /javascript:/i],
];

function walk(directory: string): string[] {
  return readdirSync(directory).flatMap((name) => {
    const path = join(directory, name);
    if (name === "node_modules" || name === "dist") return [];
    return statSync(path).isDirectory() ? walk(path) : [path];
  });
}

describe("built bundle", () => {
  it.each(forbidden)("contains no %s sink", (label, pattern) => {
    expect(bundle, label).not.toMatch(pattern);
  });

  it("stays under the brotli size budget", () => {
    const raw = Buffer.byteLength(bundle);
    const compressed = brotliCompressSync(bundle, { params: { [constants.BROTLI_PARAM_QUALITY]: constants.BROTLI_MAX_QUALITY } }).length;
    console.log(`polyui.js raw ${raw} B, brotli ${compressed} B, budget ${budget} B`);
    expect(compressed).toBeLessThan(budget);
  });

  it("has no runtime dependencies", () => {
    const manifest = JSON.parse(readFileSync(join(root, "package.json"), "utf8")) as { dependencies?: object };
    expect(manifest.dependencies ?? {}).toEqual({});
    expect(bundle).not.toMatch(/\bimport\s*[{*"']|\brequire\(/);
  });

  it("source files contain no em dash", () => {
    const files = walk(root).filter((path) => /\.(ts|mjs|css|html|json)$/.test(path) && !path.endsWith("package-lock.json"));
    const offenders = files.filter((path) => readFileSync(path, "utf8").includes("\u2014"));
    expect(offenders).toEqual([]);
  });
});
