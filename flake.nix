# Build release with: nix -L build .#release
{
  description = "The logreduce app";

  inputs = {
    # nixpkgs is tracking nixpkgs-unstable
    nixpkgs.url =
      "github:NixOS/nixpkgs/3d6ebeb283be256f008541ce2b089eb5fb0e4e01";

    flake-utils.url = "github:numtide/flake-utils";

    crane = {
      url = "github:ipetkov/crane/8b4f7a4dab2120cf41e7957a28a853f45016bd9d";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    rust-overlay = {
      url =
        "github:oxalica/rust-overlay/46dbbcaf435b0d22b149684589b9b059f73f4ffc";
      inputs = {
        nixpkgs.follows = "nixpkgs";
        flake-utils.follows = "flake-utils";
      };
    };
  };

  outputs = inputs@{ self, nixpkgs, flake-utils, crane, rust-overlay }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ (import rust-overlay) ];
        };

        rustToolchain = pkgs.rust-bin.stable.latest.default.override {
          targets = [ "x86_64-unknown-linux-musl" "wasm32-unknown-unknown" ];
        };
        craneLib = (crane.mkLib pkgs).overrideToolchain rustToolchain;

        src = pkgs.lib.cleanSourceWith {
          src = ./.; # The original, unfiltered source
          filter = path: type:
            (pkgs.lib.hasSuffix ".html" path) || (pkgs.lib.hasSuffix ".js" path)
            || (pkgs.lib.hasSuffix ".css" path)
            || (pkgs.lib.hasSuffix ".txt" path) ||
            # Default filter from crane (allow .rs files)
            (craneLib.filterCargoSources path type);
        };

        cli-info = {
          src = src;
          cargoExtraArgs = "--package=logreduce-cli";
        } // craneLib.crateNameFromCargoToml {
          cargoToml = ./crates/cli/Cargo.toml;
        };
        static-exe = craneLib.buildPackage (cli-info // {
          CARGO_BUILD_TARGET = "x86_64-unknown-linux-musl";
          CARGO_BUILD_RUSTFLAGS = "-C target-feature=+crt-static";
        });
        exe = craneLib.buildPackage cli-info;

        python = pkgs.python39.withPackages (ps:
          with ps; [
            setuptools-rust
            wheel
            scikit-learn
            numpy
            twine
            pbr
            pip
            aiohttp
            requests
            scipy
            pyyaml
            pkgs.blas
          ]);

        release = pkgs.runCommand "logreduce-release" { } ''
          echo Creating release tarball with ${static-exe}
          cd ${static-exe};
          tar --owner=0 --group=0 --mode='0755' -cf - bin/logreduce | ${pkgs.bzip2}/bin/bzip2 -9 > $out
          echo cp $out logreduce-x86_64-linux.tar.bz2
        '';

      in {
        defaultPackage = exe;
        apps.default = flake-utils.lib.mkApp {
          drv = exe;
          name = "logreduce";
        };
        devShell = craneLib.devShell {
          packages = with pkgs; [ cargo-watch trunk tailwindcss ];
          LOGREDUCE_CACHE = "1";
          UPDATE_GOLDENFILES = "1";
        };

        # nix develop .#python
        packages.python = python;
        devShells.python = pkgs.mkShell { buildInputs = [ python ]; };

        # nix build .#static
        packages.static = static-exe;
        packages.release = release;
      });
}
