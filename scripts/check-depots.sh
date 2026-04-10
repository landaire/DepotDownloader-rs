#!/usr/bin/env bash
set -euo pipefail

DD_RS="./target/release/ddl"

for depot in 731 732 2347770 2347771 2347772 2347773 2347774 2347775 2347776 2347777 2347778 2347779; do
  header=$($DD_RS files --app 730 --depot "$depot" 2>/dev/null | head -7)
  size=$(echo "$header" | grep "^Size:" | sed 's/Size:[ ]*//')
  files=$(echo "$header" | grep "^Files:" | sed 's/Files:[ ]*//')
  if [ -n "$size" ]; then
    echo "depot $depot: $size ($files files)"
  fi
done
