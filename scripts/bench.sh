#!/usr/bin/env bash
set -euo pipefail

DD_RS="./target/release/depotdownloader"
DD_CS="DepotDownloader"
TMPDIR=$(mktemp -d)
trap "rm -rf $TMPDIR" EXIT

echo "=== Benchmark: depotdownloader-rs vs DepotDownloader (C#) ==="
echo ""

# 1. List files for Spacewar (app 480, depot 481) - small depot, anonymous
echo "--- 1. List files: Spacewar (app 480, depot 481) ---"
hyperfine \
  --warmup 1 \
  --min-runs 5 \
  --export-markdown "$TMPDIR/bench_files.md" \
  -n "Rust"  "$DD_RS files --app 480 --depot 481 > /dev/null 2>&1" \
  -n "C#"    "$DD_CS -app 480 -depot 481 -manifest-only > /dev/null 2>&1"
echo ""

# 2. App info query (no download)
echo "--- 2. App info: Spacewar (app 480) ---"
hyperfine \
  --warmup 1 \
  --min-runs 5 \
  --export-markdown "$TMPDIR/bench_info.md" \
  -n "Rust"  "$DD_RS info --app 480 > /dev/null 2>&1" \
  -n "C#"    "$DD_CS -app 480 -manifest-only > /dev/null 2>&1"
echo ""

# 3. List files for a larger depot: TF2 (app 440, depot 441) - anonymous
echo "--- 3. List files: TF2 (app 440, depot 441) ---"
hyperfine \
  --warmup 1 \
  --min-runs 3 \
  --export-markdown "$TMPDIR/bench_tf2.md" \
  -n "Rust"  "$DD_RS files --app 440 --depot 441 > /dev/null 2>&1" \
  -n "C#"    "$DD_CS -app 440 -depot 441 -manifest-only > /dev/null 2>&1"
echo ""

# 4. Download Spacewar (small depot, ~1.8 MiB)
echo "--- 4. Download: Spacewar (app 480, depot 481) ---"
RUST_DL=$(mktemp -d)
CS_DL=$(mktemp -d)
hyperfine \
  --warmup 0 \
  --min-runs 3 \
  --prepare "rm -rf $RUST_DL/* $CS_DL/*" \
  --export-markdown "$TMPDIR/bench_download.md" \
  -n "Rust"  "$DD_RS download --app 480 --depot 481 --output $RUST_DL > /dev/null 2>&1" \
  -n "C#"    "$DD_CS -app 480 -depot 481 -dir $CS_DL > /dev/null 2>&1"
rm -rf "$RUST_DL" "$CS_DL"
echo ""

echo "=== Results ==="
echo ""
for f in "$TMPDIR"/bench_*.md; do
  echo "### $(basename "$f" .md)"
  cat "$f"
  echo ""
done
