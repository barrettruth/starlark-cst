{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, rust-overlay, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs { inherit system overlays; };
        toolchain = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;
        commonBuildInputs = [
          pkgs.bazel-buildtools
          pkgs.bazelisk
          pkgs.git
          pkgs.just
          pkgs.shellcheck
          pkgs.shfmt
        ];
      in
      {
        packages.default = pkgs.rustPlatform.buildRustPackage {
          pname = "starlark-cst";
          version = "0.1.0";
          src = ./.;
          cargoLock.lockFile = ./Cargo.lock;
        };

        devShells.default = pkgs.mkShell {
          buildInputs = [ toolchain ] ++ commonBuildInputs;
        };
      }
    );
}
