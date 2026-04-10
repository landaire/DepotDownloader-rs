{
  description = "depotdownloader-rs - Steam depot content downloader";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
    flake-utils.url = "github:numtide/flake-utils";
    crane.url = "github:ipetkov/crane";
  };

  outputs = {
    self,
    nixpkgs,
    rust-overlay,
    flake-utils,
    crane,
    ...
  }:
    flake-utils.lib.eachDefaultSystem (system: let
      overlays = [(import rust-overlay)];
      pkgs = import nixpkgs {inherit system overlays;};

      rustToolchainToml = fromTOML (builtins.readFile ./rust-toolchain.toml);
      inherit (rustToolchainToml.toolchain) channel components;

      rustToolchain = pkgs.rust-bin.stable.${channel}.default.override {
        extensions = components;
      };

      craneLib = (crane.mkLib pkgs).overrideToolchain rustToolchain;

      srcFilter = path: type:
        (craneLib.filterCargoSources path type)
        || (builtins.match ".*proto.*" path != null)
        || (builtins.match ".*test_data.*" path != null)
        || (builtins.match ".*captures.*" path != null)
        || (builtins.match ".*snapshots.*" path != null)
        || (builtins.match ".*scripts.*" path != null);

      commonArgs = {
        src = pkgs.lib.cleanSourceWith {
          src = ./.;
          filter = srcFilter;
        };
        strictDeps = true;

        nativeBuildInputs = with pkgs; [
          pkg-config
        ];

        buildInputs =
          []
          ++ pkgs.lib.optionals pkgs.stdenv.hostPlatform.isDarwin [
            pkgs.darwin.apple_sdk.frameworks.Security
            pkgs.darwin.apple_sdk.frameworks.SystemConfiguration
          ];

      };

      cargoArtifacts = craneLib.buildDepsOnly commonArgs;
    in {
      packages = {
        depotdownloader = craneLib.buildPackage (commonArgs
          // {
            inherit cargoArtifacts;
            cargoExtraArgs = "-p depotdownloader";
            meta.mainProgram = "depotdownloader";
          });

        default = self.packages.${system}.depotdownloader;
      };

      devShells.default = pkgs.mkShell {
        buildInputs = with pkgs; [
          rustToolchain
          pkg-config
          cargo-insta
          jujutsu
          hyperfine
          depotdownloader
        ];
      };
    });
}
