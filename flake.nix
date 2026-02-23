{
  description = "embassy flake";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    fenix = {
      url = "github:nix-community/fenix/monthly";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    naersk = {
      url = "github:nix-community/naersk";
      inputs.nixpkgs.follows = "nixpkgs";
      inputs.fenix.follows = "fenix";
    };
  };

  outputs = { self, nixpkgs, flake-utils, fenix, naersk }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [
            fenix.overlays.default 
          ];
        };
        profile = pkgs.fenix.complete;
        raspi-std-lib = pkgs.fenix.targets.aarch64-unknown-linux-musl.latest;
        aarch64-musl-cross = pkgs.pkgsCross.aarch64-multiplatform-musl;
        rust-toolchain = pkgs.fenix.combine [
          profile.rustc-unwrapped
          profile.rust-src
          profile.cargo
          profile.rustfmt
          profile.clippy
          raspi-std-lib.rust-std
        ];
      in
      {
        devShells.default =
        pkgs.mkShell {
          buildInputs = with pkgs; [
            rust-toolchain
            rust-analyzer-nightly

            # extra cargo tools
            cargo-edit
            cargo-expand
          ];

          # set the rust src for rust_analyzer
          RUST_SRC_PATH = "${rust-toolchain}/lib/rustlib/src/rust/library";
          # set c cross lib path for cross compilation
          CC_aarch64_unknown_linux_musl = "${aarch64-musl-cross.stdenv.cc}/bin/aarch64-unknown-linux-musl-cc";
        };

        packages.default = 
        (naersk.lib.${system}.override {
          cargo = rust-toolchain;
          rustc = rust-toolchain;
        }).buildPackage {
          src = ./.;
        };
      }
    );
}
