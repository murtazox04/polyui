# polyui

Flutter renderer for [PolyUI](https://github.com/murtazox04/polyui), a protocol that lets an LLM agent show real UI (cards, forms, charts, dashboards, MCP Apps) instead of plain text. The package draws PolyUI surfaces with Flutter widgets and runs the same Rust client core as the web, iOS and Android renderers, so a surface behaves the same everywhere.

## Install

```sh
flutter pub add polyui
```

The package ships the Rust core as a native asset. On iOS, Android, macOS and Linux the build hook downloads the prebuilt `libpolyui_ffi` for your target from the matching GitHub release and checks its SHA-256 against the hash pinned in this package before using it. You do not need Rust installed.

To build the core from source instead, clone the repository and point the hook at it from your app's `pubspec.yaml`:

```yaml
hooks:
  user_defines:
    polyui:
      rust_workspace: ../path/to/polyui
```

Requires Flutter 3.41 or later.

## Use

The transport is yours. Pass every server message to `receive` and send whatever the session hands to `send`:

```dart
final session = PolyuiSession.start(
  hello: const PolyuiHello(locale: 'en'),
  send: socket.add,
);
socket.listen((message) => session.receive(message as String));
socket.add(session.core.hello);
```

Then draw a surface anywhere under a `MaterialApp`:

```dart
PolyuiSurface(session: session, surface: 's1')
```

The [Flutter guide](https://github.com/murtazox04/polyui/blob/main/docs/flutter.md) covers theming, custom builders, accessibility, embedded MCP Apps and voice. The `example` app replays a bills conversation and can talk to a live `polyui serve`.

## License

Apache-2.0
