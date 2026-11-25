#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
usage="usage: pin-flutter-prebuilt.sh <version> <folder with libpolyui_ffi-<triple> libraries> [output file]"
version="${1:?$usage}"
folder="${2:?$usage}"
output="${3:-$root/flutter/polyui/hook/prebuilt.dart}"

shopt -s nullglob
libraries=("$folder"/libpolyui_ffi-*.so "$folder"/libpolyui_ffi-*.dylib)
if [ "${#libraries[@]}" -eq 0 ]; then
    echo "no libpolyui_ffi-<triple> libraries in $folder" >&2
    exit 1
fi

sha() {
    if command -v sha256sum >/dev/null; then
        sha256sum "$1" | cut -d' ' -f1
    else
        shasum -a 256 "$1" | cut -d' ' -f1
    fi
}

{
    printf "const prebuiltVersion = '%s';\n\n" "$version"
    printf "const prebuiltHashes = <String, String>{\n"
    for library in $(printf '%s\n' "${libraries[@]}" | LC_ALL=C sort); do
        printf "  '%s':\n      '%s',\n" "$(basename "$library")" "$(sha "$library")"
    done
    printf "};\n"
} >"$output"

echo "pinned ${#libraries[@]} libraries in $output"
