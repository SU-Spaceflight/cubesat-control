{
  description = "Rust for RPI 4";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = {
    nixpkgs,
    flake-utils,
    rust-overlay,
    ...
  }:
    flake-utils.lib.eachDefaultSystem (
      system: let
        overlays = [(import rust-overlay)];
        pkgs = import nixpkgs {
          inherit system overlays;
        };

        crossPkgs = pkgs.pkgsCross.aarch64-multiplatform;

        rustToolchain = pkgs.rust-bin.stable.latest.default.override {
          targets = ["aarch64-unknown-linux-gnu"];
        };
      in {
        devShells.default = pkgs.mkShell {
          buildInputs = [
            rustToolchain
          ];

          nativeBuildInputs = [
            pkgs.pkg-config
          ];

          packages = with pkgs; [
            bacon
          ];

          CARGO_BUILD_TARGET = "aarch64-unknown-linux-gnu";

          CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER = "${crossPkgs.stdenv.cc}/bin/${crossPkgs.stdenv.cc.targetPrefix}cc";

          PKG_CONFIG_ALLOW_CROSS = "1";
        };
      }
    );
}
