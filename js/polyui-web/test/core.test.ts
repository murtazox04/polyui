import { readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it, vi } from "vitest";
import { loadCore } from "../src/core";
import { createRenderer } from "../src/renderer";

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

  it("renders an extension type through a host renderer when its catalog is passed", async () => {
    const acmeHello = { ...hello, catalogs: ["core@1", "acme@1"] };
    const acme = {
      id: "acme",
      version: 1,
      types: { Badge2: { description: "A status badge.", role: "status", props: { text: { kind: "string", required: true } }, alt: "{text}" } },
    };
    const open = { v: 1, t: "ui.open", surface: "s1", seq: 1, root: "b", components: [{ id: "b", type: "acme:Badge2", props: { text: "Paid" } }], data: {} };
    const plain = await loadCore(acmeHello, wasmBytes);
    expect(JSON.parse(plain.receive(JSON.stringify(open))).ops[0]).toMatchObject({ op: "create", type: "Text" });
    plain.free();

    const core = await loadCore(acmeHello, wasmBytes, { catalogs: [acme] });
    const container = document.createElement("div");
    const sent: string[] = [];
    const renderer = createRenderer({ core, send: (message) => sent.push(message), container, sync: true });
    renderer.define("acme:Badge2", () => {
      const node = document.createElement("mark");
      return {
        element: node,
        update({ props }) {
          node.textContent = String(props["text"]);
        },
      };
    });
    renderer.receive(open);
    expect(container.querySelector("mark")?.textContent).toBe("Paid");
    expect(sent.filter((message) => message.includes("unknown_component"))).toEqual([]);
    core.free();
  });

  it("names a rejected extension catalog", async () => {
    const cases: [string, string][] = [
      ["{", "invalid JSON"],
      ['{"id":"acme"}', "must be a list of catalogs"],
      ['[{"id":"core","version":1,"types":{}}]', '[0].id: "core" is reserved'],
      ['[{"id":"acme","version":1,"types":{"Badge2":{"props":{"x":{"kind":"strng"}}}}}]', "[0].types.Badge2.props: has a prop with an unknown kind"],
    ];
    for (const [catalogs, expected] of cases) {
      await expect(loadCore(hello, wasmBytes, { catalogs })).rejects.toThrow(`PolyUI withCatalogs: an extension catalog was rejected: ${expected}`);
    }
  });
});
