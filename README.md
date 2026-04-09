## depotdownloader-rs

A translation of https://github.com/SteamRE/DepotDownloader to Rust, leveraging LLMs to do the heavy lifting.

As derivative work of SteamRE/DepotDownloader, this project is licensed under GPL v2. Please see the [LICENSE](/license) file for more details.

### Feature Parity Status


| Feature | DepotDownloader (C#) | depotdownloader-rs | Notes |
|---|---|---|---|
| Anonymous login | ✅ | ✅ | Auto-detects when no `--username` |
| Authenticated login (password + RSA) | ✅ | ✅ | OAEP-SHA1 RSA encryption |
| SteamGuard 2FA (email/authenticator) | ✅ | ✅ | Prompts for code interactively |
| QR code login | ✅ | ⚠️ | Auth API exists, CLI flow not wired |
| Token persistence (remember-password) | ✅ | ✅ | JSON file in `~/.depotdownloader/` |
| CM server discovery (API + fallback) | ✅ | ✅ | Steam Directory API with DNS fallback |
| Channel encryption (OAEP-SHA1 + AES-CBC HMAC) | ✅ | ✅ | |
| PICS access tokens + product info | ✅ | ✅ | Text KV parsing for app info |
| Depot key retrieval | ✅ | ✅ | |
| CDN server discovery | ✅ | ✅ | Service method RPC |
| CDN auth tokens | ✅ | ✅ | |
| Manifest request codes | ✅ | ✅ | |
| Manifest download + parsing (v5 protobuf) | ✅ | ✅ | |
| Manifest download + parsing (v4 binary) | ✅ | ✅ | |
| Filename decryption (AES-256-ECB/CBC) | ✅ | ✅ | Handles line-wrapped base64 |
| Chunk download (HTTP) | ✅ | ✅ | Parallel with semaphore |
| Chunk decryption (AES-256) | ✅ | ✅ | ECB IV + CBC payload |
| Chunk decompression (PKZip) | ✅ | ✅ | |
| Chunk decompression (LZMA/VZip) | ✅ | ✅ | |
| Chunk decompression (VZstd) | ✅ | ✅ | |
| Checksum verification (Adler32 zero-seed) | ✅ | ✅ | Steam's non-standard seed |
| File verification (`--verify`) | ✅ | ✅ | Per-chunk Adler32 validation |
| Delta/differential downloads | ✅ | ✅ | Chunk reuse from previous manifest |
| Depot filtering (OS/arch/language) | ✅ | ✅ | From KV `oslist`/`osarch`/`language` |
| File list filtering (`--filelist`) | ✅ | ✅ | Case-insensitive |
| Regex file filtering | ✅ | ✅ | `--file-regex` |
| Manifest caching (SHA-1 validated) | ✅ | ✅ | |
| Atomic staging writes | ✅ | ✅ | `.staging/` directory |
| Unix executable permissions | ✅ | ✅ | `EDepotFileFlag::Executable` |
| Branch fallback (try public) | ✅ | ✅ | |
| Depotfromapp detection | ✅ | ✅ | Parsed from KV, shown in info |
| Workshop pubfile download (HTTP) | ✅ | ✅ | Direct URL download |
| Workshop depot download | ✅ | ⚠️ | hcontent path stubbed |
| UGC download | ✅ | ⚠️ | Stub |
| Beta/branch passwords | ✅ | ❌ | `encryptedmanifests` parsed but password flow not implemented |
| Account access verification (license check) | ✅ | ❌ | |
| Lancache detection + proxy | ✅ | ❌ | |
| Multi-depot file deduplication | ✅ | ❌ | |
| Build number directory tracking | ✅ | ❌ | |
| Custom login ID (`--loginid`) | ✅ | ❌ | CLI flag exists, not wired |

### New Features / Library Changes


| Feature | Description |
|---|---|
| `info` command | Full app overview: branches, depots, manifests in one view |
| `manifests` command | List all depot manifests for a branch (or filter to one depot) |
| `--format json` on all commands | Machine-readable JSON output for scripting/pipelines |
| `--raw` flag on `files` | Show encrypted filenames without attempting decryption |
| `--capture` flag | Record incoming network packets to JSON for replay testing |
| Transport abstraction | Pluggable `Transport` trait (TCP, Recording, Replay) |
| Replay test harness | Integration tests using real captured Steam server packets |
| Snapshot tests (insta) | TOML snapshots for manifest and KV parsing |
| Text KV parser | Valve text KeyValue format (PICS app info uses this, not binary) |
| `DD_COMPAT=1` mode | Legacy flat-arg CLI compatible with original DepotDownloader |
| Branch metadata | Build ID, time updated, password required, description |
| Encrypted manifest detection | Shows `encryptedmanifests` entries with `encrypted: true` |
| Checksum newtypes | `Sha1Hash`, `Adler32`, `SteamAdler32`, `Crc32` — prevents mixing |
| Depot config persistence | Tracks installed manifest IDs for delta downloads across runs |
