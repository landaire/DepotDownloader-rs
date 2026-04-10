#!/usr/bin/env bash
set -euo pipefail

DD_RS="./target/release/ddl"
DD_CS="DepotDownloader"
BENCH_DIR="/mnt/g/dev/depotdownloader-rs/.bench"
RUST_DL="$BENCH_DIR/dl_rust"
CS_DL="$BENCH_DIR/dl_cs"
mkdir -p "$RUST_DL" "$CS_DL"

echo "--- Download: Spacewar (1.8 MiB) ---"
hyperfine \
  --warmup 1 \
  --min-runs 5 \
  --prepare "rm -rf $RUST_DL/* $CS_DL/*" \
  -n "Rust"  "$DD_RS download --app 480 --depot 481 --output $RUST_DL > /dev/null 2>&1" \
  -n "C#"    "$DD_CS -app 480 -depot 481 -dir $CS_DL > /dev/null 2>&1"

rm -rf "$BENCH_DIR"
