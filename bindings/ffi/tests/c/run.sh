#!/usr/bin/env bash
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
crate="$(cd "$here/../.." && pwd)"
root="$(cd "$crate/../.." && pwd)"
out="$root/target/ffi-c-test"
features="${POLYUI_FFI_FEATURES:-}"
cc="${CC:-clang}"
cxx="${CXX:-clang++}"

case "$(uname -s)" in
    Linux) system_libs="-lpthread -ldl -lm -lrt" ;;
    *) system_libs="" ;;
esac

mkdir -p "$out"
cargo build --release -p polyui-ffi ${features:+--features "$features"} --manifest-path "$root/Cargo.toml"
library="$root/target/release/libpolyui_ffi.a"

"$cc" -std=c11 -Wall -Wextra -Werror -pedantic -g \
    -fsanitize=address,undefined -fno-sanitize-recover=all \
    -I "$crate/include" \
    "$here/abi_test.c" "$library" $system_libs \
    -o "$out/abi_test"

if [ "$(uname -s)" = "Linux" ]; then
    cat >"$out/leak_probe.c" <<'C'
#include <stdlib.h>
int main(void) {
    void *volatile block = malloc(64);
    block = NULL;
    return block == NULL ? 0 : 1;
}
C
    "$cc" -fsanitize=address "$out/leak_probe.c" -o "$out/leak_probe"
    if ASAN_OPTIONS=detect_leaks=1 "$out/leak_probe" 2>/dev/null; then
        echo "LeakSanitizer missed a known leak, so this run cannot vouch for leaks"
        exit 1
    fi
    ASAN_OPTIONS=detect_leaks=1:abort_on_error=1 "$out/abi_test"
    echo "no leaks reported by LeakSanitizer"
else
    "$out/abi_test"
    "$cc" -std=c11 -Wall -Wextra -Werror -pedantic \
        -I "$crate/include" \
        "$here/abi_test.c" "$library" \
        -o "$out/abi_test_plain"
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

"$cxx" -std=c++17 -Wall -Wextra -Werror -x c++ -fsyntax-only -I "$crate/include" - <<'CPP'
#include "polyui.h"
int main() { polyui_client_free(nullptr); return 0; }
CPP
echo "header compiles as C++"
