#!/usr/bin/env bash
set -euo pipefail

DD_RS="./target/release/depotdownloader"

echo "CS2 depot 2347770 file size distribution:"
echo ""

# Get file listing and compute sizes per extension/pattern
$DD_RS files --app 730 --depot 2347770 --bytes 2>/dev/null | tail -n +8 | while IFS= read -r line; do
  echo "$line"
done | awk '
{
  # Last two fields are size and chunks, everything before is filename
  n = NF
  size = $(n-1)
  name = $1

  # Get extension
  if (match(name, /\.[^.\/]+$/)) {
    ext = substr(name, RSTART)
  } else {
    ext = "(none)"
  }

  ext_size[ext] += size
  ext_count[ext] += 1
  total += size
}
END {
  print "Extension breakdown:"
  for (ext in ext_size) {
    printf "  %-15s %6d files  %12.1f MiB\n", ext, ext_count[ext], ext_size[ext]/1048576
  }
  printf "\nTotal: %.1f GiB\n", total/1073741824
}
'

echo ""
echo "Files matching .vpk:"
$DD_RS files --app 730 --depot 2347770 --bytes 2>/dev/null | tail -n +8 | grep '\.vpk' | head -20
