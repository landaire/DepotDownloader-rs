"""Demo: list files in Spacewar (app 480, depot 481) using the steam-ffi nanobind module."""
import os, sys
from pathlib import Path

# Add the Rust cdylib to the DLL search path
_lib_dir = str(Path(__file__).resolve().parent.parent.parent / "target" / "release")
if sys.platform == "win32":
    os.add_dll_directory(_lib_dir)

from steam_ffi_ext import (
    CdnClient,
    CmServerList,
    DepotManifest,
    Runtime,
    SteamClient,
)

APP_ID = 480
DEPOT_ID = 481
MANIFEST_ID = 3183503801510301321


def main():
    rt = Runtime()

    servers = CmServerList.fetch(rt)
    print(f"Got {len(servers)} CM servers")

    client = SteamClient.connect(rt, servers)
    client.login_anonymous(rt)
    print("Logged in anonymously")

    tokens = client.get_access_tokens(rt, [APP_ID])
    app_infos = client.get_product_info(rt, tokens)
    print(f"Got product info for {len(app_infos)} app(s)")

    cdn_servers = client.get_cdn_servers(rt)
    request_code = client.get_manifest_request_code(rt, APP_ID, DEPOT_ID, MANIFEST_ID, "public")

    cdn = CdnClient()
    manifest = cdn.download_manifest(rt, cdn_servers, 0, DEPOT_ID, MANIFEST_ID, request_code)

    if manifest.filenames_encrypted:
        key = client.get_depot_key(rt, DEPOT_ID, APP_ID)
        manifest.decrypt_filenames(key)

    print(f"\nDepot {DEPOT_ID} - {manifest.file_count} files, {manifest.total_uncompressed_size:,} bytes\n")
    print(f"{'FILENAME':<50} {'SIZE':>12} {'CHUNKS':>8}")
    for i in range(manifest.file_count):
        name = manifest.file_name(i)
        size = manifest.file_size(i)
        chunks = manifest.file_chunk_count(i)
        print(f"{name:<50} {size:>12,} {chunks:>8}")


if __name__ == "__main__":
    main()
