#!/usr/bin/env bash
set -euo pipefail

DD_RS="./target/release/ddl"

echo "Files matching maps/de_dust2:"
$DD_RS files --app 730 --depot 2347770 --bytes 2>/dev/null | tail -n +8 | grep 'maps/de_dust2'

echo ""
echo "Files matching maps/de_mirage:"
$DD_RS files --app 730 --depot 2347770 --bytes 2>/dev/null | tail -n +8 | grep 'maps/de_mirage'

echo ""
echo "Files matching maps/de_inferno:"
$DD_RS files --app 730 --depot 2347770 --bytes 2>/dev/null | tail -n +8 | grep 'maps/de_inferno'

echo ""
echo "Files matching maps/de_nuke:"
$DD_RS files --app 730 --depot 2347770 --bytes 2>/dev/null | tail -n +8 | grep 'maps/de_nuke'

echo ""
echo "Files matching maps/de_anubis:"
$DD_RS files --app 730 --depot 2347770 --bytes 2>/dev/null | tail -n +8 | grep 'maps/de_anubis'
