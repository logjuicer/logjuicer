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
            (pkgs.lib.hasSuffix ".html" path)
            || (pkgs.lib.hasSuffix ".config.js" path)
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

        web-info = {
          src = src;
          cargoExtraArgs = "--package=logreduce-web";
        } // craneLib.crateNameFromCargoToml {
          cargoToml = ./crates/web/Cargo.toml;
        };
        web-package = {
          name = "logreduce-web";
          description = "Web Interface for logreduce";
          license = "Apache-2.0";
          homepage = "https://github.com/logreduce/logreduce";
          repository = {
            type = "git";
            url = "https://github.com/logreduce/logreduce";
          };
          keywords = [ "anomaly-detection" "machine-learning" "wasm" "yew" ];
          version = web-info.version;
          files = [
            "README.md"
            "LICENSE"
            "logreduce-web.js"
            "logreduce-web.wasm"
            "logreduce-web.css"
          ];
        };
        web-package-json = pkgs.writeTextFile {
          name = "package.json";
          text = builtins.toJSON web-package;
        };
        cargoArtifactsWasm = craneLib.buildDepsOnly (web-info // {
          doCheck = false;
          CARGO_BUILD_TARGET = "wasm32-unknown-unknown";
        });
        web = craneLib.buildTrunkPackage (web-info // {
          cargoArtifacts = cargoArtifactsWasm;
          trunkIndexPath = "./index.html";
          # Start the build relative to the crate to take the tailwind.config.js into account.
          preBuild = "cd crates/web";
          buildInputs = [ pkgs.tailwindcss ];
          # Fixup the dist output for a publishable package.
          postInstall = ''
            rm $out/index.html
            mv $out/*.js $out/logreduce-web.js
            mv $out/*.wasm $out/logreduce-web.wasm
            mv $out/*.css $out/logreduce-web.css
            cp ${self}/LICENSE $out
            cp ${self}/crates/web/README.md $out
            cp ${web-package-json} $out/package.json
          '';
        });

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
        packages.web = web;
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
