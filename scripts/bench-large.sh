#!/usr/bin/env bash
set -euo pipefail

DD_RS="./target/release/ddl"
DD_CS="DepotDownloader"
RESULTS=$(mktemp -d)

# Use a regex to get de_dust2 + de_mirage + de_nuke = ~2.1 GiB
FILELIST="$RESULTS/filelist.txt"
printf 'regex:maps/de_(dust2|mirage|nuke)\n' > "$FILELIST"
echo "Filelist: $(cat "$FILELIST")"
echo ""

# Verify file count and size
echo "Matching files (Rust):"
$DD_RS files --app 730 --depot 2347770 --bytes 2>/dev/null | tail -n +8 | grep -E 'maps/de_(dust2|mirage|nuke)'
echo ""

echo "--- Download: CS2 de_dust2+mirage+nuke (app 730, depot 2347770, ~2.1 GiB) ---"
RUST_DL=$(mktemp -d)
CS_DL=$(mktemp -d)
hyperfine \
  --warmup 0 \
  --min-runs 2 \
  --prepare "rm -rf $RUST_DL/* $CS_DL/*" \
  --export-markdown "$RESULTS/bench_download_2gb.md" \
  --show-output \
  -n "Rust"  "$DD_RS download --app 730 --depot 2347770 --filelist $FILELIST --output $RUST_DL 2>&1" \
  -n "C#"    "$DD_CS -app 730 -depot 2347770 -filelist $FILELIST -dir $CS_DL 2>&1"

echo ""
echo "=== Result ==="
cat "$RESULTS/bench_download_2gb.md"

rm -rf "$RUST_DL" "$CS_DL" "$RESULTS"
