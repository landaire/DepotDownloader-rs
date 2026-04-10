#!/usr/bin/env bash
set -euo pipefail

DD_RS="./target/release/depotdownloader"

# Count .cfg files in CS2 depot
echo "Counting .cfg files in CS2 depot 2347770..."
$DD_RS files --app 730 --depot 2347770 2>/dev/null | grep -c '\.cfg' || echo "0 .cfg files"

# Count .vdf files (might be more common)
echo "Counting .vdf files..."
$DD_RS files --app 730 --depot 2347770 2>/dev/null | grep -c '\.vdf' || echo "0 .vdf files"

# Count .txt files
echo "Counting .txt files..."
$DD_RS files --app 730 --depot 2347770 2>/dev/null | grep -c '\.txt' || echo "0 .txt files"

# Show some sample filenames
echo ""
echo "Sample filenames:"
$DD_RS files --app 730 --depot 2347770 2>/dev/null | head -20
