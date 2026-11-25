#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
usage="usage: check-pins.sh <version> <folder with the draft release files>"
version="${1:?$usage}"
folder="${2:?$usage}"

expected="$(mktemp)"
trap 'rm -f "$expected"' EXIT
bash "$root/scripts/release/pin-flutter-prebuilt.sh" "$version" "$folder" "$expected" >/dev/null
if ! diff -u "$root/flutter/polyui/hook/prebuilt.dart" "$expected"; then
    echo "flutter/polyui/hook/prebuilt.dart does not match the draft release" >&2
    exit 1
fi

zip_sum="$(sha256sum "$folder/PolyUICore.xcframework.zip" | cut -d' ' -f1)"
if ! grep -q "checksum: \"$zip_sum\"" "$root/Package.swift"; then
    echo "Package.swift does not pin PolyUICore.xcframework.zip ($zip_sum)" >&2
    exit 1
fi
if ! grep -q "releases/download/v$version/PolyUICore.xcframework.zip" "$root/Package.swift"; then
    echo "Package.swift does not point at the v$version release" >&2
    exit 1
fi
echo "the pins in this commit match the draft release"
