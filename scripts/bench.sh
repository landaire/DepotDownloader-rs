#!/usr/bin/env bash
set -euo pipefail

DD_RS="./target/release/depotdownloader"
DD_CS="DepotDownloader"
BENCH_DIR="/mnt/g/dev/depotdownloader-rs/.bench"
mkdir -p "$BENCH_DIR"
RESULTS="$BENCH_DIR/results"
mkdir -p "$RESULTS"
trap "rm -rf $BENCH_DIR" EXIT

echo "=== Benchmark: depotdownloader-rs vs DepotDownloader (C#) ==="
echo "Download target: $BENCH_DIR"
echo ""

# 1. List files for Spacewar (app 480, depot 481) - small depot, anonymous
echo "--- 1. List files: Spacewar (8 files, 1.8 MiB) ---"
hyperfine \
  --warmup 1 \
  --min-runs 5 \
  --export-markdown "$RESULTS/bench_1_files_spacewar.md" \
  -n "Rust"  "$DD_RS files --app 480 --depot 481 > /dev/null 2>&1" \
  -n "C#"    "$DD_CS -app 480 -depot 481 -manifest-only > /dev/null 2>&1"
echo ""

# 2. App info query (no download)
echo "--- 2. App info: Spacewar ---"
hyperfine \
  --warmup 1 \
  --min-runs 5 \
  --export-markdown "$RESULTS/bench_2_info_spacewar.md" \
  -n "Rust"  "$DD_RS info --app 480 > /dev/null 2>&1" \
  -n "C#"    "$DD_CS -app 480 -manifest-only > /dev/null 2>&1"
echo ""

# 3. List files for TF2 (app 440, depot 441) - medium manifest
echo "--- 3. List files: TF2 (1189 files, 29.4 GiB) ---"
hyperfine \
  --warmup 1 \
  --min-runs 3 \
  --export-markdown "$RESULTS/bench_3_files_tf2.md" \
  -n "Rust"  "$DD_RS files --app 440 --depot 441 > /dev/null 2>&1" \
  -n "C#"    "$DD_CS -app 440 -depot 441 -manifest-only > /dev/null 2>&1"
echo ""

# 4. List files for CS2 (app 730, depot 2347770) - large manifest
echo "--- 4. List files: CS2 (2841 files, 50.4 GiB) ---"
hyperfine \
  --warmup 1 \
  --min-runs 3 \
  --export-markdown "$RESULTS/bench_4_files_cs2.md" \
  -n "Rust"  "$DD_RS files --app 730 --depot 2347770 > /dev/null 2>&1" \
  -n "C#"    "$DD_CS -app 730 -depot 2347770 -manifest-only > /dev/null 2>&1"
echo ""

# 5. Download Spacewar (~1.8 MiB)
echo "--- 5. Download: Spacewar (1.8 MiB) ---"
RUST_DL="$BENCH_DIR/dl_rust"
CS_DL="$BENCH_DIR/dl_cs"
mkdir -p "$RUST_DL" "$CS_DL"
hyperfine \
  --warmup 0 \
  --min-runs 3 \
  --prepare "rm -rf $RUST_DL/* $CS_DL/*" \
  --export-markdown "$RESULTS/bench_5_download_1_8mb.md" \
  -n "Rust"  "$DD_RS download --app 480 --depot 481 --output $RUST_DL > /dev/null 2>&1" \
  -n "C#"    "$DD_CS -app 480 -depot 481 -dir $CS_DL > /dev/null 2>&1"
rm -rf "$RUST_DL" "$CS_DL"
echo ""

# 6. Download CS2 de_dust2 + de_mirage + de_nuke (~2.1 GiB, 6 files)
echo "--- 6. Download: CS2 3 maps (2.1 GiB) ---"
FILELIST="$BENCH_DIR/filelist.txt"
printf 'regex:maps/de_(dust2|mirage|nuke)\n' > "$FILELIST"
RUST_DL="$BENCH_DIR/dl_rust"
CS_DL="$BENCH_DIR/dl_cs"
mkdir -p "$RUST_DL" "$CS_DL"
hyperfine \
  --warmup 0 \
  --min-runs 2 \
  --prepare "rm -rf $RUST_DL/* $CS_DL/*" \
  --export-markdown "$RESULTS/bench_6_download_2_1gb.md" \
  -n "Rust"  "$DD_RS download --app 730 --depot 2347770 --filelist $FILELIST --output $RUST_DL > /dev/null 2>&1" \
  -n "C#"    "$DD_CS -app 730 -depot 2347770 -filelist $FILELIST -dir $CS_DL > /dev/null 2>&1"
rm -rf "$RUST_DL" "$CS_DL"
echo ""

echo "=== Results ==="
echo ""
for f in "$RESULTS"/bench_*.md; do
  echo "### $(basename "$f" .md | sed 's/bench_[0-9]*_//; s/_/ /g')"
  cat "$f"
  echo ""
done
