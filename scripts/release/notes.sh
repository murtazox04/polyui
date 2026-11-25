#!/usr/bin/env bash
set -euo pipefail

version="${1:?usage: notes.sh <version>}"

cat <<NOTES
PolyUI $version.

## Install

| Where | How |
|---|---|
| Rust server and CLI | \`cargo install polyui\`, or a \`polyui-$version-<target>\` archive below |
| Rust crates | \`polyui-core\`, \`polyui-chart\`, \`polyui-server\`, \`polyui-query\`, \`polyui-decode\` on crates.io |
| Python | \`pip install polyui==$version\` |
| Node | \`npm install @polyui/node@$version\` |
| Web renderer | \`npm install @polyui/web@$version\` |
| Swift (iOS 17, Mac Catalyst 17) | Swift Package Manager: \`https://github.com/murtazox04/polyui\`, from \`$version\` |
| Android | \`implementation("io.github.murtazox04:polyui-android:$version")\` |
| Flutter | \`flutter pub add polyui:$version\` |

## Files

- \`polyui-$version-<target>\`: the \`polyui\` command line tool (reference server, tool schemas, show_ui checks).
- \`PolyUICore.xcframework.zip\`: the Rust client core for the Swift package. Package.swift pins its checksum.
- \`libpolyui_ffi-<target>\`: the Rust client core for the Flutter package. The package pins the SHA-256 of each file and refuses any other.
- \`SHA256SUMS\`: checksums of every file. Each file also has a build provenance attestation: \`gh attestation verify <file> --repo murtazox04/polyui\`.
NOTES
