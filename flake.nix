# Build release with: nix -L build .#release
{
  description = "The logreduce app";

  inputs = {
    # nixpkgs is tracking nixpkgs-unstable
    nixpkgs.url =
      "github:NixOS/nixpkgs/b11ced7a9c1fc44392358e337c0d8f58efc97c89";

    flake-utils.url = "github:numtide/flake-utils";

    crane = {
      url = "github:ipetkov/crane/9dae37b4a545f05aa70a2f048428c5196690c5a4";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    rust-overlay = {
      url =
        "github:oxalica/rust-overlay/b48a7e5dab1b472dd9c9ee9053401489dbb4d6fc";
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
            || (pkgs.lib.hasSuffix ".sql" path)
            || (pkgs.lib.hasSuffix ".json" path)
            || (pkgs.lib.hasSuffix ".txt" path) ||
            # Default filter from crane (allow .rs files)
            (craneLib.filterCargoSources path type);
        };

        base-info =
          craneLib.crateNameFromCargoToml { cargoToml = ./Cargo.toml; };

        cli-info = base-info // {
          src = src;
          pname = "logreduce-cli";
          cargoExtraArgs = "--package=logreduce-cli";
        };
        exe = craneLib.buildPackage
          (cli-info // { cargoArtifacts = craneLib.buildDepsOnly cli-info; });

        cli-static-info = cli-info // {
          CARGO_BUILD_TARGET = "x86_64-unknown-linux-musl";
          CARGO_BUILD_RUSTFLAGS = "-C target-feature=+crt-static";
        };
        static-exe = craneLib.buildPackage (cli-static-info // {
          cargoArtifacts = craneLib.buildDepsOnly cli-static-info;
        });

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
            # remove hash from import url
            sed -e 's/logreduce.*bg\.wasm/logreduce-web.wasm/' -i $out/logreduce-web.js
            mv $out/*.wasm $out/logreduce-web.wasm
            mv $out/*.css $out/logreduce-web.css
            cp ${self}/LICENSE $out
            cp ${self}/crates/web/README.md $out
            cp ${web-package-json} $out/package.json
          '';
        });

        api-info = base-info // {
          src = src;
          pname = "logreduce-api";
          cargoExtraArgs = "--package=logreduce-web-service";
        };
        api = craneLib.buildPackage (api-info // {
          # Start the build relative to the crate to take the sqlx migrations into account.
          preBuild = "cd crates/web-service";
          cargoArtifacts = craneLib.buildDepsOnly api-info;
        });

        container = pkgs.dockerTools.streamLayeredImage {
          name = "ghcr.io/logreduce/logreduce";
          contents = [ api web ];
          tag = "latest";
          created = "now";
          extraCommands = "mkdir 1777 data";
          config.Entrypoint = [ "logreduce-api" ];
          config.Env = [ "LOGREDUCE_ASSETS=${web}/" ];
          config.Labels = {
            "org.opencontainers.image.source" =
              "https://github.com/logreduce/logreduce";
          };
        };

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
        packages.api = api;
        packages.web = web;
        # use with:
        # $(nix build .#container) | podman load
        # or publish directly with:
        # $(nix build .#container) | gzip --fast | skopeo copy docker-archive:/dev/stdin docker://ghcr.io/logreduce/logreduce:latest
        packages.container = container;
        apps.default = flake-utils.lib.mkApp {
          drv = exe;
          name = "logreduce";
        };
        devShell = craneLib.devShell {
          packages = with pkgs; [
            rust-analyzer
            cargo-watch
            trunk
            tailwindcss
            wasm-pack
            sqlx-cli
            sqlite
          ];
          LOGREDUCE_CACHE = "1";
          UPDATE_GOLDENFILES = "1";
          # `cargo sqlx prepare` needs an absolute path (`database create` and `migrate run` don't)
          shellHook = ''
            if test -d crates/web-service/data; then
              export DATABASE_URL="sqlite://$(pwd)/crates/web-service/data/logreduce.sqlite?mode=rwc";
            else
              export DATABASE_URL="sqlite://$(pwd)/data/logreduce.sqlite?mode=rwc";
            fi
          '';
        };

        # nix develop .#python
        packages.python = python;
        devShells.python = pkgs.mkShell { buildInputs = [ python ]; };

        # nix build .#static
        packages.static = static-exe;
        packages.release = release;
      });
}
