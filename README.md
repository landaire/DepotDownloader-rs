## depotdownloader-rs

A translation of https://github.com/SteamRE/DepotDownloader to Rust, leveraging LLMs to do the heavy lifting.

As derivative work of SteamRE/DepotDownloader, this project is licensed under GPL v2. Please see the [LICENSE](/license) file for more details.

See [FEATURES.md](FEATURES.md) for feature parity status and new features.

### Usage

Browse app info (branches, depots, manifests):

```
$ ddl info --app 480
App 480

Branches:
  NAME                      BUILD                UPDATED FLAGS
  previous                 316058 2017-08-23 17:48:02 UTC
  public                  3538192 2019-02-06 21:52:51 UTC

Depots:
  ID           NAME                           OS                   ARCH
  229006
  481

Manifests for branch 'public':
  depot 229006     -> --
  depot 481        -> 3183503801510301321
```

List files in a depot:

```
$ ddl files --app 480 --depot 481
Depot:    481
Manifest: 3183503801510301321
Created:  2019-02-06 21:51:33 UTC
Size:     1.82 MiB
Files:    8

FILENAME                                                             SIZE   CHUNKS
D3D9VRDistort.cso                                                   576 B        1
DejaVuSans.ttf                                                 703.96 KiB        1
DejaVuSans.txt                                                   2.76 KiB        1
SteamworksExample.exe                                          374.00 KiB        1
controller.vdf                                                   1.53 KiB        1
installscript.vdf                                                   514 B        1
sdkencryptedappticket.dll                                      558.28 KiB        1
steam_api.dll                                                  219.78 KiB        1
```

Plain output for piping:

```
$ ddl files --app 480 --depot 481 --format plain
D3D9VRDistort.cso
DejaVuSans.ttf
DejaVuSans.txt
SteamworksExample.exe
controller.vdf
installscript.vdf
sdkencryptedappticket.dll
steam_api.dll
```

Download depot content:

```
$ ddl download --app 480 --depot 481 --output ./spacewar

$ ls -la spacewar/
     576 D3D9VRDistort.cso
  720856 DejaVuSans.ttf
    2828 DejaVuSans.txt
  382976 SteamworksExample.exe
    1569 controller.vdf
     514 installscript.vdf
  571680 sdkencryptedappticket.dll
  225056 steam_api.dll
```

Authenticated login (for paid games):

```
$ ddl download --app 552990 --depot 552993 --username myuser --remember-password --output ./grow
Password for myuser:
Confirm login on your Steam mobile app...
Logged in successfully as myuser
```

### FFI bindings

The `steam-ffi` crate exposes the full Steam client API over a C ABI using [rust-diplomat](https://github.com/rust-diplomat/diplomat). This lets you use the library from C, C++, Python, or any language with C FFI support.

```bash
cargo build --release -p steam-ffi
cargo install diplomat-tool
diplomat-tool cpp crates/steam-ffi/bindings/cpp --entry crates/steam-ffi/src/lib.rs
```

A working Python example using [nanobind](https://github.com/wjakob/nanobind) is in `examples/python/`. It connects to Steam, downloads a manifest, decrypts filenames, and lists files:

```python
from steam_ffi_ext import Runtime, CmServerList, SteamClient, CdnClient

rt = Runtime()
servers = CmServerList.fetch(rt)
client = SteamClient.connect(rt, servers)
client.login_anonymous(rt)

tokens = client.get_access_tokens(rt, [480])
app_infos = client.get_product_info(rt, tokens)
cdn_servers = client.get_cdn_servers(rt)

cdn = CdnClient()
manifest = cdn.download_manifest(rt, cdn_servers, 0, 481, 3183503801510301321, 0)

if manifest.filenames_encrypted:
    key = client.get_depot_key(rt, 481, 480)
    manifest.decrypt_filenames(key)

for i in range(manifest.file_count):
    print(f"{manifest.file_name(i)}: {manifest.file_size(i)} bytes")
```

See [crates/steam-ffi/README.md](crates/steam-ffi/README.md) for full details.

### Benchmarks

| Benchmark | Rust | C# | Speedup |
|---|---|---|---|
| **List files: Spacewar** (8 files, 1.8 MiB) | 818ms | 3.22s | **3.9x** |
| **App info: Spacewar** | 929ms | 2.65s | **2.9x** |
| **List files: TF2** (1189 files, 29.4 GiB) | 835ms | 3.14s | **3.8x** |
| **List files: CS2** (2841 files, 50.4 GiB) | 846ms | 3.37s | **4.0x** |
| **Download: Spacewar** (1.8 MiB) | 1.00s | 3.48s | **3.5x** |
| **Download: CS2 3 maps** (2.1 GiB) | 7.35s | 10.61s | **1.4x** |
