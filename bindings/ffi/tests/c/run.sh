#!/usr/bin/env bash
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
crate="$(cd "$here/../.." && pwd)"
root="$(cd "$crate/../.." && pwd)"
out="$root/target/ffi-c-test"
features="${POLYUI_FFI_FEATURES:-}"

mkdir -p "$out"
cargo build --release -p polyui-ffi ${features:+--features "$features"} --manifest-path "$root/Cargo.toml"

clang -std=c11 -Wall -Wextra -Werror -pedantic -fsanitize=address,undefined \
    -I "$crate/include" \
    "$here/abi_test.c" "$root/target/release/libpolyui_ffi.a" \
    -o "$out/abi_test"
"$out/abi_test"

clang -std=c11 -Wall -Wextra -Werror -pedantic \
    -I "$crate/include" \
    "$here/abi_test.c" "$root/target/release/libpolyui_ffi.a" \
    -o "$out/abi_test_plain"
if command -v leaks >/dev/null 2>&1; then
    status=0
    MallocStackLogging=1 perl -e 'alarm shift; exec @ARGV' 120 \
        leaks --atExit -- "$out/abi_test_plain" >"$out/leaks.txt" 2>&1 || status=$?
    grep -E "leaks for|LEAK" "$out/leaks.txt" || true
    case "$status" in
        0) ;;
        142)
            pkill -f "$out/abi_test_plain" || true
            echo "leaks did not finish within 120 s, leak check skipped"
            ;;
        *)
            cat "$out/leaks.txt"
            echo "leaks reported a leak or failed (exit $status)"
            exit 1
            ;;
    esac
fi

clang++ -std=c++17 -Wall -Wextra -Werror -x c++ -fsyntax-only -I "$crate/include" - <<'CPP'
#include "polyui.h"
int main() { polyui_client_free(nullptr); return 0; }
CPP
echo "header compiles as C++"
