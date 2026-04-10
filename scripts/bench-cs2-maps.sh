#!/usr/bin/env bash
set -euo pipefail

DD_RS="./target/release/ddl"
DD_CS="DepotDownloader"
BENCH_DIR="/mnt/g/dev/depotdownloader-rs/.bench"
RESULTS="$BENCH_DIR/results"
FILELIST="$BENCH_DIR/filelist.txt"
RUST_DL="$BENCH_DIR/dl_rust"
CS_DL="$BENCH_DIR/dl_cs"
mkdir -p "$RESULTS" "$RUST_DL" "$CS_DL"
trap "rm -rf $BENCH_DIR" EXIT

printf 'regex:maps/de_(dust2|mirage|nuke)\n' > "$FILELIST"

# Use timeout to cap each run at 60s -- if Steam rate-limits us,
# the run gets killed rather than dragging the mean up to 131s.
echo "--- Download: CS2 3 maps (2.1 GiB) ---"
hyperfine \
  --warmup 0 \
  --min-runs 5 \
  --ignore-failure \
  --prepare "rm -rf $RUST_DL $CS_DL && mkdir -p $RUST_DL $CS_DL" \
  --export-markdown "$RESULTS/bench.md" \
  -n "Rust"  "timeout 120 $DD_RS download --app 730 --depot 2347770 --filelist $FILELIST --output $RUST_DL > /dev/null 2>&1" \
  -n "C#"    "timeout 120 $DD_CS -app 730 -depot 2347770 -filelist $FILELIST -dir $CS_DL > /dev/null 2>&1"

echo ""
cat "$RESULTS/bench.md"
