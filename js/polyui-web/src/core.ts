import startWasm, { Client } from "../wasm/polyui_core.js";
import type { Json, PolyuiCore } from "./types";

export type CoreSource = string | URL | Request | Response | Promise<Response> | BufferSource | WebAssembly.Module;

export type HelloMessage = string | { readonly [key: string]: Json };

export type ExtensionCatalogs = string | readonly { readonly [key: string]: Json }[];

export interface CoreOptions {
  readonly catalogs?: ExtensionCatalogs;
}

export interface WasmCore extends PolyuiCore {
  free(): void;
}

function jsonText(value: HelloMessage | ExtensionCatalogs): string {
  return typeof value === "string" ? value : JSON.stringify(value);
}

export async function loadCore(hello: HelloMessage, source?: CoreSource, options: CoreOptions = {}): Promise<WasmCore> {
  await startWasm(source === undefined ? undefined : { module_or_path: source });
  const helloText = jsonText(hello);
  return options.catalogs === undefined ? new Client(helloText) : Client.withCatalogs(helloText, jsonText(options.catalogs));
}
