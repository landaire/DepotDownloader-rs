## depotdownloader-rs

A translation of https://github.com/SteamRE/DepotDownloader to Rust, leveraging LLMs to do the heavy lifting.

As derivative work of SteamRE/DepotDownloader, this project is licensed under GPL v2. Please see the [LICENSE](/license) file for more details.

See [FEATURES.md](FEATURES.md) for feature parity status and new features.

### Usage

Browse app info (branches, depots, manifests):

```
$ depotdownloader info --app 480
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
$ depotdownloader files --app 480 --depot 481
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
$ depotdownloader files --app 480 --depot 481 --format plain
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
$ depotdownloader download --app 480 --depot 481 --output ./spacewar

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
$ depotdownloader download --app 552990 --depot 552993 --username myuser --remember-password --output ./grow
Password for myuser:
Confirm login on your Steam mobile app...
Logged in successfully as myuser
```

### Benchmarks

| Benchmark | Rust | C# | Speedup |
|---|---|---|---|
| **List files: Spacewar** (8 files, 1.8 MiB) | 982ms | 1.15s | **1.2x** |
| **App info: Spacewar** | 427ms | 2.88s | **6.8x** |
| **List files: TF2** (1189 files, 29.4 GiB) | 912ms | 2.99s | **3.3x** |
| **List files: CS2** (2841 files, 50.4 GiB) | 814ms | 4.21s | **5.2x** |
| **Download: Spacewar** (1.8 MiB) | 1.08s | 3.52s | **3.3x** |
| **Download: CS2 3 maps** (2.1 GiB) | 5.12s | 9.48s | **1.9x** |
