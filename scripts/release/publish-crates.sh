#!/usr/bin/env bash
set -euo pipefail

version="${1:?usage: publish-crates.sh <version>}"
crates=(polyui-core polyui-chart polyui-query polyui-server polyui-decode polyui)

for crate in "${crates[@]}"; do
    status="$(curl -s -o /dev/null -w '%{http_code}' -A "polyui-release (https://github.com/murtazox04/polyui)" \
        "https://crates.io/api/v1/crates/$crate/$version")"
    if [ "$status" = 200 ]; then
        echo "$crate $version is already on crates.io, skipping"
        continue
    fi
    cargo publish --locked -p "$crate"
done
