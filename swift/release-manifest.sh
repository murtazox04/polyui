#!/usr/bin/env bash
set -euo pipefail

usage="usage: release-manifest.sh <version> <xcframework zip checksum> | --local <xcframework path>"
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root="$(cd "$here/.." && pwd)"

if [ "${1:-}" = "--local" ]; then
    artifact="$(printf '            path: "%s"' "${2:?$usage}")"
else
    version="${1:?$usage}"
    checksum="${2:?$usage}"
    if ! [[ "$checksum" =~ ^[0-9a-f]{64}$ ]]; then
        echo "the checksum must be the 64 hex characters printed by swift package compute-checksum" >&2
        exit 1
    fi
    artifact="$(printf '            url: "https://github.com/murtazox04/polyui/releases/download/v%s/PolyUICore.xcframework.zip",\n            checksum: "%s"' "$version" "$checksum")"
fi

cat >"$root/Package.swift" <<MANIFEST
// swift-tools-version: 6.2
import PackageDescription

let package = Package(
    name: "PolyUI",
    platforms: [.iOS(.v17), .macCatalyst(.v17)],
    products: [
        .library(name: "PolyUI", targets: ["PolyUI"])
    ],
    targets: [
        .binaryTarget(
            name: "PolyUICore",
$artifact
        ),
        .target(
            name: "PolyUI",
            dependencies: ["PolyUICore"],
            path: "swift/PolyUI/Sources/PolyUI",
            swiftSettings: [.enableUpcomingFeature("ExistentialAny")]
        ),
    ],
    swiftLanguageModes: [.v6]
)
MANIFEST
