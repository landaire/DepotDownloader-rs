"""Integration test: connect to Steam, list Spacewar files with decrypted names."""
import os, sys
from pathlib import Path

_lib_dir = str(Path(__file__).resolve().parent.parent.parent.parent / "target" / "release")
if sys.platform == "win32":
    os.add_dll_directory(_lib_dir)

from steam_ffi_ext import CdnClient, CmServerList, Runtime, SteamClient

APP_ID = 480
DEPOT_ID = 481
MANIFEST_ID = 3183503801510301321

EXPECTED_FILES = {
    "D3D9VRDistort.cso",
    "DejaVuSans.ttf",
    "DejaVuSans.txt",
    "SteamworksExample.exe",
    "controller.vdf",
    "installscript.vdf",
    "sdkencryptedappticket.dll",
    "steam_api.dll",
}


def test_spacewar_file_listing():
    rt = Runtime()
    servers = CmServerList.fetch(rt)
    assert len(servers) > 0

    # Retry connection across servers
    client = None
    for i in range(min(len(servers), 5)):
        try:
            client = SteamClient.connect(rt, servers, i)
            break
        except RuntimeError:
            continue
    assert client is not None

    client.login_anonymous(rt)

    tokens = client.get_access_tokens(rt, [APP_ID])
    app_infos = client.get_product_info(rt, tokens)
    assert len(app_infos) >= 1

    cdn_servers = client.get_cdn_servers(rt)
    assert len(cdn_servers) > 0

    request_code = client.get_manifest_request_code(rt, APP_ID, DEPOT_ID, MANIFEST_ID, "public")

    cdn = CdnClient()
    manifest = cdn.download_manifest(rt, cdn_servers, 0, DEPOT_ID, MANIFEST_ID, request_code)

    if manifest.filenames_encrypted:
        key = client.get_depot_key(rt, DEPOT_ID, APP_ID)
        manifest.decrypt_filenames(key)

    assert manifest.file_count == len(EXPECTED_FILES)
    assert manifest.total_uncompressed_size > 0
    assert not manifest.filenames_encrypted

    actual_files = {manifest.file_name(i) for i in range(manifest.file_count)}
    assert actual_files == EXPECTED_FILES

    for i in range(manifest.file_count):
        assert manifest.file_size(i) > 0
        assert manifest.file_chunk_count(i) > 0
