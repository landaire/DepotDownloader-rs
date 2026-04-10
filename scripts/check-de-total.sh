#!/usr/bin/env bash
set -euo pipefail
DD_RS="./target/release/depotdownloader"
$DD_RS files --app 730 --depot 2347770 --bytes 2>/dev/null | tail -n +8 | grep 'maps/de_' | awk '{total += $(NF-1); count++} END {printf "Total de_ maps: %.2f GiB (%d files)\n", total/1073741824, count}'
