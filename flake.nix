{
  description = "Cargo project, Rust version from Cargo.toml";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, flake-utils, rust-overlay }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ rust-overlay.overlays.default ];
        };

        # ← вот тут читаем Cargo.toml
        cargoToml = builtins.fromTOML (builtins.readFile ./Cargo.toml);

        # rust-version обычно пишут как "1.85", а rust-overlay хочет "1.85.0"
        raw = cargoToml.package.rust-version;
        rustVersion =
          if builtins.match "[0-9]+\\.[0-9]+" raw != null
          then "${raw}.0"
          else raw;

        rustToolchain = pkgs.rust-bin.stable.${rustVersion}.minimal;

        rustPlatform = pkgs.makeRustPlatform {
          cargo = rustToolchain;
          rustc = rustToolchain;
        };
      in {
        packages.default = rustPlatform.buildRustPackage {
          pname   = cargoToml.package.name;
          version = cargoToml.package.version;
          src = ./.;
          cargoLock.lockFile = ./Cargo.lock;
        };

        devShells.default = pkgs.mkShell {
          packages = [ rustToolchain ];
        };
      });
}
