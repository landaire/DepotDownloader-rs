## steam-ffi

C/C++ FFI bindings for the `steam` and `steam-client` crates, generated with [rust-diplomat](https://github.com/rust-diplomat/diplomat).

### Building

```bash
cargo build --release -p steam-ffi
```

This produces `target/release/libsteam_ffi.so` (Linux), `libsteam_ffi.dylib` (macOS), or `steam_ffi.dll` (Windows).

### Generating bindings

Install diplomat-tool:

```bash
cargo install diplomat-tool
```

Generate C or C++ headers:

```bash
diplomat-tool c crates/steam-ffi/bindings/c --entry crates/steam-ffi/src/lib.rs
diplomat-tool cpp crates/steam-ffi/bindings/cpp --entry crates/steam-ffi/src/lib.rs
```

### Using from C/C++

Link against the built cdylib and include the generated headers:

```c
#include "SteamClient.h"
#include "Runtime.h"
#include "CmServerList.h"

Runtime* rt = Runtime_new().ok;
CellId* cell = CellId_new(0);
CmServerList* servers = CmServerList_fetch(rt, cell).ok;
SteamClient* client = SteamClient_connect(rt, servers, 0).ok;
SteamClient_login_anonymous(client, rt, cell);

// ... query apps, download manifests ...

SteamClient_destroy(client);
CmServerList_destroy(servers);
CellId_destroy(cell);
Runtime_destroy(rt);
```

### Using from Python

See `examples/python/` for a complete working example using [nanobind](https://github.com/wjakob/nanobind). The diplomat C++ headers are wrapped into a native Python extension with proper classes and automatic lifetime management:

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

To build the Python extension locally:

```bash
cd examples/python
STEAM_FFI_LIB_DIR=/path/to/target/release \
STEAM_FFI_BINDINGS_DIR=/path/to/crates/steam-ffi/bindings/cpp \
pip install -e .
```

Pre-built wheels are attached to each GitHub release.

### Using from other languages

The cdylib exposes a standard C ABI. Any language with C FFI support (Go, Ruby, Zig, etc.) can load it. Use the generated C headers as the reference for function signatures.
