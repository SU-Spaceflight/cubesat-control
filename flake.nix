{
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
        pkgs = import nixpkgs {inherit system overlays;};
        rustToolchain = pkgs.rust-bin.stable.latest.default.override {
          targets = ["aarch64-unknown-linux-gnu"];
        };
      in {
        devShells.default = pkgs.mkShell {
          nativeBuildInputs = [
            (pkgs.writeScriptBin "rustup" ''
              #!/usr/bin/env bash
              # Fake rustup to trick cross-rs into seeing what it wants to see

              if [ "$1" = "toolchain" ] && [ "$2" = "list" ]; then
                echo "stable-x86_64-unknown-linux-gnu (default)"
                exit 0
              fi

              if [ "$1" = "target" ] && [ "$2" = "list" ]; then
                echo "aarch64-unknown-linux-gnu (installed)"
                echo "x86_64-unknown-linux-gnu (installed)"
                exit 0
              fi

              if [ "$1" = "run" ]; then
                shift 2
                exec "$@"
              fi

              exit 0
            '')
            rustToolchain
            pkgs.python3
            pkgs.opencv
            pkgs.pkg-config
            pkgs.clang
            pkgs.mold
            pkgs.openssl
          ];

          packages = with pkgs; [
            bacon
            cargo-cross
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
        };
      }
    );
}
