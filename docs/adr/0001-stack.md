# 1. Rust core everywhere, thin native renderers

Date: 2026-11-24
Status: accepted

## What we needed to decide

PolyUI has to be very light, very fast, smooth to use, safe, and easy to customize. It must run on the web, inside mobile WebViews, in native iOS and Android apps, in Flutter, and in voice-only clients. Servers that host it are written in Rust, Python and Node.

## Decision

1. **One Rust core** (`polyui-core`) holds every rule: protocol types, catalog checks, surface state, bindings, `alt` and `say` text, URL sanitizing, voice matching, and the UI summary for the model. It has no I/O and no async runtime, so it compiles to wasm and to native libraries without changes.
2. **Renderers are thin and native to each platform.** On the web that is plain custom elements written in TypeScript, with no framework. iOS gets SwiftUI, Android gets Compose, Flutter gets widgets. They all receive the same render ops from the core.
3. **The server is Rust** (`polyui-server`, axum). Python and Node hosts use the core through PyO3 and napi-rs.
4. **Charts are compiled by Rust** (`polyui-chart`) into SVG plus a hit map, so they are small, need no chart library, and are tappable on every platform.
5. **One crate per binding.** The workspace has a core crate and separate crates for wasm, Python and Node.

## Why not the alternatives

| Option | Why we did not pick it |
|---|---|
| Draw the DOM from Rust (Leptos, Dioxus, Yew) | In js-framework-benchmark (Chrome 158) they score 1.16 to 1.53 against 1.03 for plain DOM code, ship 47 to 115 kB against 2.5 kB, and paint first 4 to 11 times later. The cost is the string copy across the wasm boundary on every DOM call. |
| Draw on a canvas from Rust (egui, Iced, Slint web, Makepad) | No screen reader support on the web, broken soft keyboard and IME on phones, no find in page, no browser zoom. Accessibility is a hard requirement. |
| React renderer | React adds about 50 kB and scores 1.55 in the same benchmark. Custom elements work inside React, Vue or plain pages, so React apps lose nothing. |
| Vega-Lite for charts | Vega, Vega-Lite and vega-embed together are about 283 kB gzip and need a JS runtime on the server to prerender. |
| One crate with feature flags for every binding | tokenizers, swc, oxc and polars all split bindings into their own crates. Mixing pyo3, napi and wasm-bindgen in one crate breaks linking when features merge. |

## Costs we accept

- Each new component has to be drawn once per platform. The conformance fixtures in `tests/conformance` keep the renderers honest.
- The wasm core has to stay small or it slows first paint in WebViews. The budget is 60 kB brotli for the client build, checked in CI. The build is now 59.0 kB brotli (168.0 kB raw, 68.6 kB gzip), down from 73.3 kB. The Embed support added about 0.7 kB of that. To get there, the wasm crate turns on the `client-lite` feature of `polyui-core`, and only when it builds for wasm. That feature swaps `serde_json::Value` for a small JSON value of our own that keeps keys in a plain ordered list instead of a hash map, with its own parser and writer, and leaves out the parts only servers need (validation, summary, tools, stream). The parser follows serde_json's rules and error messages and the writer matches its output byte for byte; tests check both against serde_json, and the client fixtures give the same JSON text with and without the feature. The feature cannot be combined with the default features. `wasm-opt -O1` gives a smaller brotli file than `-Oz` here, so the build uses it.
- Wasm needs `wasm-unsafe-eval` in the page's CSP.
