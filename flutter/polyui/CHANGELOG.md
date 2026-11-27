## 0.1.8

- Cancel on a confirm sheet tells the server at once (`ui.dismiss`) when the server accepts the `dismiss` feature.
- An embedded MCP App can send `ui/update-model-context` when the server accepts the `app_context` feature.

## 0.1.7

- An app call that the server asks a person about waits in the renderer's confirm sheet; the session waits 90 seconds for an app call answer instead of 40.

## 0.1.6

- An MCP App inside `embed:Embed` can call `tools/call` and `resources/read` on its own MCP server through the PolyUI server, when the server accepts the `app_calls` feature.

## 0.1.5

- The example app comes back to the same session after a dropped connection: its cards keep working and a pending confirmation is shown again.

## 0.1.4

- An MCP App inside `embed:Embed` is told the app's light or dark theme and hears when it changes.
- The example's microphone shows the unavailable notice when the recognizer does not start, instead of staying silent.

## 0.1.3

- Built from the same core as PolyUI 0.1.3. No change to the Flutter API.

## 0.1.2

- Built from the same core as PolyUI 0.1.2. No change to the Flutter API.

## 0.1.1

- Built from the same core as PolyUI 0.1.1. No change to the Flutter API.

## 0.1.0

First release.

- Draws every type in the `core`, `chart` and `embed` catalogs with Flutter widgets.
- Runs the PolyUI Rust client core over `dart:ffi`, shipped as a native asset with prebuilt libraries for iOS, Android, macOS and Linux.
- Embeds MCP Apps in a sandboxed web view.
- Voice: the core's spoken summaries and voice command matching, ready to connect to any speech engine.
