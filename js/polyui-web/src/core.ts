import startWasm, { Client } from "../wasm/polyui_core.js";
import type { Json, PolyuiCore } from "./types";

export type CoreSource = string | URL | Request | Response | Promise<Response> | BufferSource | WebAssembly.Module;

export type HelloMessage = string | { readonly [key: string]: Json };

export interface WasmCore extends PolyuiCore {
  free(): void;
}

export async function loadCore(hello: HelloMessage, source?: CoreSource): Promise<WasmCore> {
  await startWasm(source === undefined ? undefined : { module_or_path: source });
  return new Client(typeof hello === "string" ? hello : JSON.stringify(hello));
}
