{
  description = "Osmium - vZDC Backend API Platform";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, rust-overlay, flake-utils, ... }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs {
          inherit system overlays;
        };

        rustToolchain = pkgs.rust-bin.stable.latest.default.override {
          extensions = [ "rust-src" "rust-analyzer" "clippy" "rustfmt" ];
        };

        darwinDeps = pkgs.lib.optionals pkgs.stdenv.isDarwin (with pkgs; [
          apple-sdk_15
          libiconv
        ]);

      in
      {
        devShells.default = pkgs.mkShell {
          name = "osmium-dev";

          buildInputs = with pkgs; [
            rustToolchain
            postgresql_16
            sqlx-cli
            docker
            docker-compose
            git
            pkg-config
            openssl
            cargo-watch
            cargo-nextest
          ] ++ darwinDeps;

          env = {
            OPENSSL_DIR = "${pkgs.openssl.dev}";
            OPENSSL_LIB_DIR = "${pkgs.openssl.out}/lib";
            OPENSSL_INCLUDE_DIR = "${pkgs.openssl.dev}/include";
            PKG_CONFIG_PATH = "${pkgs.openssl.dev}/lib/pkgconfig";
            RUST_BACKTRACE = "1";
          };

          shellHook = ''
            echo ""
            echo "Osmium Development Environment"
            echo "==============================="
            echo "Rust: $(rustc --version)"
            echo "Cargo: $(cargo --version)"
            echo "sqlx-cli: $(sqlx --version)"
            echo ""
            echo "Quick start:"
            echo "  docker compose up -d postgres"
            echo "  cargo run"
            echo ""
          '';
        };
      });
}
