#!/usr/bin/env bash
set -euo pipefail

usage="usage: build-rust.sh android | host <output directory>"
mode="${1:?$usage}"

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root="$(cd "$here/../.." && pwd)"
features="${POLYUI_FFI_FEATURES-lite}"
min_sdk="${POLYUI_MIN_SDK:-26}"
target_dir="${CARGO_TARGET_DIR:-$root/target}"

if [ -f "$HOME/.cargo/env" ]; then
    source "$HOME/.cargo/env"
fi

feature_args=()
if [ -n "$features" ]; then
    feature_args=(--features "$features")
fi

build_android() {
    local triples=(aarch64-linux-android armv7-linux-androideabi x86_64-linux-android)
    local installed
    installed="$(rustup target list --installed)"
    for triple in "${triples[@]}"; do
        if ! grep -qx "$triple" <<<"$installed"; then
            rustup target add "$triple"
        fi
    done
    if ! cargo ndk --version >/dev/null 2>&1; then
        echo "cargo-ndk is missing, install it with: cargo install cargo-ndk" >&2
        exit 1
    fi
    cargo ndk -t arm64-v8a -t armeabi-v7a -t x86_64 --platform "$min_sdk" \
        build --release --lib -p polyui-ffi ${feature_args[@]+"${feature_args[@]}"} --manifest-path "$root/Cargo.toml"
    for triple in "${triples[@]}"; do
        touch "$target_dir/$triple/release/libpolyui_ffi.a"
    done
}

build_host() {
    local output="${1:?$usage}"
    : "${JAVA_HOME:?set JAVA_HOME to a JDK so the JNI headers can be found}"
    cargo build --release --lib -p polyui-ffi ${feature_args[@]+"${feature_args[@]}"} --manifest-path "$root/Cargo.toml"
    mkdir -p "$output"
    local sources=("$here/../polyui/src/main/cpp/polyui_jni.c" "$target_dir/release/libpolyui_ffi.a")
    local flags=(-std=c11 -O2 -Wall -Wextra -Werror -fPIC -fvisibility=hidden
        -I"$JAVA_HOME/include" -I"$root/bindings/ffi/include")
    case "$(uname -s)" in
        Darwin)
            cc "${flags[@]}" -I"$JAVA_HOME/include/darwin" -dynamiclib "${sources[@]}" \
                -o "$output/libpolyui_jni.dylib"
            ;;
        Linux)
            cc "${flags[@]}" -I"$JAVA_HOME/include/linux" -shared "${sources[@]}" \
                -Wl,--exclude-libs,ALL -lpthread -ldl -lm -o "$output/libpolyui_jni.so"
            ;;
        *)
            echo "host builds work on macOS and Linux only" >&2
            exit 1
            ;;
    esac
}

case "$mode" in
    android) build_android ;;
    host) build_host "${2:-}" ;;
    *)
        echo "$usage" >&2
        exit 1
        ;;
esac
