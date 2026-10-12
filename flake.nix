{
  description = "Rust (Native + RPI 4 Cross) and Python";

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
            pkgs.python3
            pkgs.opencv
          ];

          nativeBuildInputs = [
            pkgs.pkg-config
            pkgs.clang
            pkgs.mold
          ];

          packages = with pkgs; [
            bacon
            python3Packages.pip
            python3Packages.virtualenv
          ];

          shellHook = ''
            if [ ! -d ".venv" ]; then
              echo "Creating Python virtual environment in .venv..."
              python -m venv .venv
            fi

            source .venv/bin/activate

            if [ -f "requirements.txt" ]; then
              echo "Installing Python packages from requirements.txt..."
              pip install -r requirements.txt --quiet
            else
              echo "No requirements.txt found. Skipping pip install."
            fi
          '';
          LIBCLANG_PATH = "${pkgs.libclang.lib}/lib";

          BINDGEN_EXTRA_CLANG_ARGS = "-isystem ${pkgs.llvmPackages.libclang.lib}/lib/clang/${pkgs.libclang.version}/include";

          CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER = "${crossPkgs.stdenv.cc}/bin/${crossPkgs.stdenv.cc.targetPrefix}cc";
          PKG_CONFIG_ALLOW_CROSS = "1";
        };
      }
    );
}
