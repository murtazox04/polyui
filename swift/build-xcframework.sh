#!/usr/bin/env bash
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root="$(cd "$here/.." && pwd)"
output="$here/PolyUI/PolyUICore.xcframework"
stage="$root/target/xcframework"
features="${POLYUI_FFI_FEATURES:-lite}"
device=(aarch64-apple-ios)
simulator=(aarch64-apple-ios-sim x86_64-apple-ios)
catalyst=(aarch64-apple-ios-macabi x86_64-apple-ios-macabi)
targets=("${device[@]}" "${simulator[@]}" "${catalyst[@]}")

export IPHONEOS_DEPLOYMENT_TARGET="${IPHONEOS_DEPLOYMENT_TARGET:-17.0}"

if [ -f "$HOME/.cargo/env" ]; then
    source "$HOME/.cargo/env"
fi

sysroot="$(cd "$root" && rustc --print sysroot)"
for target in "${targets[@]}"; do
    if [ ! -d "$sysroot/lib/rustlib/$target/lib" ]; then
        (cd "$root" && rustup target add "$target")
    fi
done

rm -rf "$stage" "$output"
mkdir -p "$stage/headers"
cp "$root/bindings/ffi/include/polyui.h" "$stage/headers/"
cat >"$stage/headers/module.modulemap" <<'MAP'
module CPolyUI {
    header "polyui.h"
    export *
}
MAP

feature_args=()
if [ -n "$features" ]; then
    feature_args=(--features "$features")
fi

for target in "${targets[@]}"; do
    cargo build --release --lib -p polyui-ffi --target "$target" "${feature_args[@]}" --manifest-path "$root/Cargo.toml"
    mkdir -p "$stage/$target"
    xcrun strip -S "$root/target/$target/release/libpolyui_ffi.a" -o "$stage/$target/libpolyui_ffi.a"
done

combine() {
    local name="$1"
    shift
    mkdir -p "$stage/$name"
    local inputs=()
    for target in "$@"; do
        inputs+=("$stage/$target/libpolyui_ffi.a")
    done
    xcrun lipo -create "${inputs[@]}" -output "$stage/$name/libpolyui_ffi.a"
}

combine device "${device[@]}"
combine simulator "${simulator[@]}"
combine catalyst "${catalyst[@]}"

libraries=()
for slice in device simulator catalyst; do
    libraries+=(-library "$stage/$slice/libpolyui_ffi.a" -headers "$stage/headers")
done

xcodebuild -create-xcframework "${libraries[@]}" -output "$output" >/dev/null

human() {
    awk -v bytes="$1" 'BEGIN { printf "%.2f MB", bytes / 1048576 }'
}

echo "PolyUICore.xcframework (features: ${features:-default})"
for target in "${targets[@]}"; do
    raw="$(stat -f%z "$root/target/$target/release/libpolyui_ffi.a")"
    stripped="$(stat -f%z "$stage/$target/libpolyui_ffi.a")"
    echo "  $target: $(human "$raw") as built, $(human "$stripped") without debug info"
done
for slice in device simulator catalyst; do
    echo "  $slice slice: $(xcrun lipo -archs "$stage/$slice/libpolyui_ffi.a")"
done
total="$(du -sk "$output" | awk '{ print $1 * 1024 }')"
echo "  xcframework on disk: $(human "$total")"
echo "  written to $output"
