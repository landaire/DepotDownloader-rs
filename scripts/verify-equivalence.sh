#!/usr/bin/env bash
set -euo pipefail

DD_RS="./target/release/ddl"
DD_CS="DepotDownloader"

BENCH_DIR="/mnt/g/dev/depotdownloader-rs/.bench/equiv"
mkdir -p "$BENCH_DIR"
RUST_DIR="$BENCH_DIR/rust"
CS_DIR="$BENCH_DIR/cs"
mkdir -p "$RUST_DIR" "$CS_DIR"
trap "rm -rf $BENCH_DIR" EXIT

APP=480
DEPOT=481

echo "=== Download equivalence check: app $APP depot $DEPOT ==="
echo ""

echo "Downloading with Rust..."
$DD_RS download --app $APP --depot $DEPOT --output "$RUST_DIR" 2>&1 | tail -5
echo ""

echo "Downloading with C#..."
$DD_CS -app $APP -depot $DEPOT -dir "$CS_DIR" 2>&1 | tail -5
echo ""

# C# puts files under depots/<depot_id>/<manifest_id>/
CS_CONTENT=$(find "$CS_DIR" -type f | head -1)
CS_BASE=$(dirname "$CS_CONTENT")
# Walk up until we find the actual content root (directory containing the app files)
# C# layout: <dir>/depots/<depot_id>/<manifest_id>/files...
# Find it by looking for a known file
CS_BASE=$(find "$CS_DIR" -name "SteamworksExample.exe" -printf '%h\n' 2>/dev/null | head -1)

# Rust layout: <dir>/<depot_id>/files...
RUST_BASE=$(find "$RUST_DIR" -name "SteamworksExample.exe" -printf '%h\n' 2>/dev/null | head -1)

if [[ -z "$CS_BASE" || -z "$RUST_BASE" ]]; then
    echo "FAIL: Could not find content root in one or both downloads"
    echo "  Rust dir contents:"
    find "$RUST_DIR" -type f | sort
    echo "  C# dir contents:"
    find "$CS_DIR" -type f | sort
    exit 1
fi

echo "Rust content root: $RUST_BASE"
echo "C# content root:   $CS_BASE"
echo ""

# Compare file trees (exclude metadata dirs)
echo "--- File tree comparison ---"
RUST_TREE=$(cd "$RUST_BASE" && find . -type f -not -path './.depotdownloader/*' -not -path './.DepotDownloader/*' | sort)
CS_TREE=$(cd "$CS_BASE" && find . -type f -not -path './.depotdownloader/*' -not -path './.DepotDownloader/*' | sort)

if [[ "$RUST_TREE" == "$CS_TREE" ]]; then
    echo "PASS: File trees are identical"
else
    echo "FAIL: File trees differ"
    diff <(echo "$RUST_TREE") <(echo "$CS_TREE") || true
    exit 1
fi
echo ""

# Compare file hashes
echo "--- File hash comparison ---"
RUST_HASHES=$(cd "$RUST_BASE" && find . -type f -not -path './.depotdownloader/*' -not -path './.DepotDownloader/*' | sort | xargs sha256sum)
CS_HASHES=$(cd "$CS_BASE" && find . -type f -not -path './.depotdownloader/*' -not -path './.DepotDownloader/*' | sort | xargs sha256sum)

if [[ "$RUST_HASHES" == "$CS_HASHES" ]]; then
    echo "PASS: All file hashes match"
else
    echo "FAIL: File hashes differ"
    diff <(echo "$RUST_HASHES") <(echo "$CS_HASHES") || true
    exit 1
fi
echo ""

# Print summary
FILE_COUNT=$(echo "$RUST_TREE" | wc -l)
echo "=== RESULT: $FILE_COUNT files downloaded, all identical ==="
echo ""
echo "File listing with hashes:"
echo "$RUST_HASHES"
