import { readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it, vi } from "vitest";
import { loadCore } from "../src/core";

const wasmBytes = readFileSync(join(import.meta.dirname, "..", "wasm", "polyui_core_bg.wasm"));
const hello = { v: 1, t: "hello", catalogs: ["core@1"], modalities: ["screen"], locale: "en" };

describe("loadCore", () => {
  it("compiles a fetched module with instantiateStreaming and reuses it", async () => {
    const streaming = vi
      .spyOn(WebAssembly, "instantiateStreaming")
      .mockImplementation(async (source, imports) => WebAssembly.instantiate(await (await source).arrayBuffer(), imports));
    const response = new Response(wasmBytes, { headers: { "content-type": "application/wasm" } });
    const first = await loadCore(hello, Promise.resolve(response));
    const second = await loadCore(JSON.stringify(hello));
    expect(streaming).toHaveBeenCalledTimes(1);
    const open = { v: 1, t: "ui.open", surface: "s1", seq: 1, root: "t", components: [{ id: "t", type: "Text", props: { text: "Hi" } }] };
    expect(JSON.parse(first.receive(JSON.stringify(open)))).toMatchObject({ surface: "s1", fallback: "Hi" });
    expect(JSON.parse(second.say("s1"))).toEqual([]);
    first.free();
    second.free();
    streaming.mockRestore();
  });

  it("throws a readable error for a bad hello and for bad JSON arguments", async () => {
    await expect(loadCore({ v: 1, t: "hello", modalities: [] }, wasmBytes)).rejects.toThrow("PolyUI could not start a client from this hello: invalid hello: modalities must not be empty");
    const core = await loadCore(hello, wasmBytes);
    expect(() => core.setValue("s1", "/a", "{")).toThrow("PolyUI setValue: the value is not valid JSON");
    expect(() => core.fire("s1", "b", "on_press", "{")).toThrow("PolyUI fire: the detail is not valid JSON");
    expect(core.fire("s1", "b", "on_press", "[1]")).toBeNull();
    expect(core.confirm("s1", "a", "n")).toBeNull();
    expect(core.matchVoice("s1", "yes")).toBeNull();
    core.free();
  });
});
