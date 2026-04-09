#!/usr/bin/env bash
set -euo pipefail

# Syncs .proto files from SteamTracking/Protobufs into crates/steam/proto/steam/
# Run this from the repository root.

REPO_URL="https://github.com/SteamTracking/Protobufs"
BRANCH="master"
DEST="crates/steam/proto/steam"

# Proto files we actually need (subset of the full SteamTracking repo)
PROTOS=(
    "steam/steammessages_base.proto"
    "steam/steammessages_unified_base.steamclient.proto"
    "steam/steammessages_auth.steamclient.proto"
    "steam/steammessages_clientserver_login.proto"
    "steam/steammessages_clientserver.proto"
    "steam/steammessages_clientserver_2.proto"
    "steam/steammessages_clientserver_appinfo.proto"
    "steam/steammessages_contentsystem.steamclient.proto"
    "steam/steammessages_publishedfile.steamclient.proto"
    "steam/content_manifest.proto"
    "steam/enums.proto"
    "steam/enums_clientserver.proto"
    "steam/encrypted_app_ticket.proto"
    "steam/enums_productinfo.proto"
)

echo "Syncing proto files from $REPO_URL ..."

rm -rf "$DEST"
mkdir -p "$DEST"

for proto in "${PROTOS[@]}"; do
    filename=$(basename "$proto")
    echo "  Fetching $filename ..."
    curl -sL "$REPO_URL/raw/$BRANCH/$proto" -o "$DEST/$filename"
done

echo "Done. Proto files synced to $DEST/"
