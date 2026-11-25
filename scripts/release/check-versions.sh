#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
version="${1:?usage: check-versions.sh <version>}"

if ! [[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.]+)?$ ]]; then
    echo "$version is not a semantic version such as 0.1.0" >&2
    exit 1
fi

found=0
check() {
    local file="$1" actual="$2"
    if [ "$actual" = "$version" ]; then
        echo "ok    $file: $actual"
    else
        echo "WRONG $file: '$actual', expected $version" >&2
        found=1
    fi
}

workspace="$(sed -n '/^\[workspace.package\]/,/^\[/s/^version = "\(.*\)"/\1/p' "$root/Cargo.toml")"
check "Cargo.toml [workspace.package]" "$workspace"
while read -r name actual; do
    check "Cargo.toml [workspace.dependencies] $name" "$actual"
done < <(sed -nE 's/^(polyui-[a-z]+) = \{.*version = "([^"]+)".*/\1 \2/p' "$root/Cargo.toml")

json_version() {
    node -p "require(process.argv[1]).version" "$1"
}
check "bindings/node/package.json" "$(json_version "$root/bindings/node/package.json")"
check "js/polyui-web/package.json" "$(json_version "$root/js/polyui-web/package.json")"
check "flutter/polyui/pubspec.yaml" "$(sed -n 's/^version: //p' "$root/flutter/polyui/pubspec.yaml")"
check "flutter/polyui/hook/prebuilt.dart" "$(sed -n "s/^const prebuiltVersion = '\(.*\)';/\1/p" "$root/flutter/polyui/hook/prebuilt.dart")"
check "android/gradle.properties" "$(sed -n 's/^VERSION_NAME=//p' "$root/android/gradle.properties")"

if [ "$found" -ne 0 ]; then
    echo "bump every file above to $version before releasing" >&2
    exit 1
fi
