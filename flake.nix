# Build release with: nix -L build .#release
{
  description = "The LogJuicer app";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";

    flake-utils.url = "github:numtide/flake-utils";

    crane.url = "github:ipetkov/crane";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
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
          pname = "logjuicer-cli";
          cargoExtraArgs = "--package=logjuicer-cli";
        };
        exe = craneLib.buildPackage
          (cli-info // { cargoArtifacts = craneLib.buildDepsOnly cli-info; });

        cli-static-info = cli-info // {
          CARGO_BUILD_TARGET = "x86_64-unknown-linux-musl";
          CARGO_BUILD_RUSTFLAGS = "-C target-feature=+crt-static";
        };
        static-exe = craneLib.buildPackage (cli-static-info // {
          cargoArtifacts = craneLib.buildDepsOnly cli-static-info;
          strictDeps = true;
        });

        web-info = base-info // {
          src = src;
          pname = "logjuicer-web";
          cargoExtraArgs = "--package=logjuicer-web";
        };
        web-package = {
          name = "logjuicer-web";
          description = "Web Interface for logjuicer";
          license = "Apache-2.0";
          homepage = "https://github.com/logjuicer/logjuicer";
          repository = {
            type = "git";
            url = "https://github.com/logjuicer/logjuicer";
          };
          keywords = [ "anomaly-detection" "machine-learning" "wasm" "yew" ];
          version = web-info.version;
          files = [
            "README.md"
            "LICENSE"
            "LogJuicer.svg"
            "logjuicer-web.js"
            "logjuicer-web.wasm"
            "logjuicer-web.css"
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
        mk-web = api_client:
          craneLib.buildTrunkPackage (web-info // {
            wasm-bindgen-cli =  pkgs.buildWasmBindgenCli rec {
              src = pkgs.fetchCrate {
                pname = "wasm-bindgen-cli";
                version = "0.2.90";
                hash = "sha256-X8+DVX7dmKh7BgXqP7Fp0smhup5OO8eWEhn26ODYbkQ=";
              };
              cargoDeps = pkgs.rustPlatform.fetchCargoVendor {
                inherit src;
                inherit (src) pname version;
                hash = "sha256-ti2jA4+GlczghJUOzvaUklomjpuanCNuk0sTNCUgk8k=";
              };
            };
            cargoArtifacts = cargoArtifactsWasm;
            # Start the build relative to the crate to take the tailwind.config.js into account.
            preBuild = "cd crates/web";
            trunkExtraBuildArgs =
              if api_client then "" else "--no-default-features=true";
            buildInputs = [ pkgs.tailwindcss ];
            # Fixup the dist output for a publishable package.
            postInstall = ''
              rm $out/index.html
              mv $out/*.js $out/logjuicer-web.js
              # remove hash from import url
              sed -e 's/logjuicer.*bg\.wasm/logjuicer-web.wasm/' -i $out/logjuicer-web.js
              mv $out/*.wasm $out/logjuicer-web.wasm
              mv $out/*.css $out/logjuicer-web.css
              cp ${self}/doc/LogJuicer.svg $out
              cp ${self}/LICENSE $out
              cp ${self}/crates/web/README.md $out
              cp ${web-package-json} $out/package.json
            '';
          });
        web-standalone = mk-web false;
        web = mk-web true;

        api-info = base-info // {
          src = src;
          pname = "logjuicer-api";
          cargoExtraArgs = "--package=logjuicer-web-service";
        };
        api = craneLib.buildPackage (api-info // {
          # Start the build relative to the crate to take the sqlx migrations into account.
          preBuild = "cd crates/web-service";
          cargoArtifacts = craneLib.buildDepsOnly api-info;
        });

        container-name = "ghcr.io/logjuicer/logjuicer";

        container = pkgs.dockerTools.streamLayeredImage {
          name = container-name;
          contents = [ api web ];
          tag = "latest";
          created = "now";
          extraCommands = "mkdir 1777 data";
          config.Entrypoint = [ "logjuicer-api" ];
          config.Env = [ "LOGJUICER_ASSETS=${web}/" ];
          config.Labels = {
            "org.opencontainers.image.source" =
              "https://github.com/logjuicer/logjuicer";
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

        release = pkgs.runCommand "logjuicer-release" { } ''
          echo Creating release tarball with ${static-exe}
          cd ${static-exe};
          tar --owner=0 --group=0 --mode='0755' -cf - bin/logjuicer | ${pkgs.bzip2}/bin/bzip2 -9 > $out
          echo cp $out logjuicer-x86_64-linux.tar.bz2
        '';

        publish-container-release =
          pkgs.writeShellScriptBin "logjuicer-release" ''
            set -e
            export PATH=$PATH:${pkgs.gzip}/bin:${pkgs.skopeo}/bin
            IMAGE="docker://${container-name}"

            echo "Logging to registry..."
            echo $GH_TOKEN | skopeo login --username $GH_USERNAME --password-stdin ghcr.io

            echo "Building and publishing the image..."
            ${container} | gzip --fast | skopeo copy docker-archive:/dev/stdin $IMAGE:${api-info.version}

            echo "Tagging latest"
            skopeo copy $IMAGE:${api-info.version} $IMAGE:latest
          '';

      in {
        defaultPackage = exe;
        packages.api = api;
        packages.web = web-standalone;
        packages.web_api = web;
        # use with:
        # $(nix build .#container) | podman load
        packages.container = container;
        apps.default = flake-utils.lib.mkApp {
          drv = exe;
          name = "logjuicer";
        };
        apps.publish-container-release =
          flake-utils.lib.mkApp { drv = publish-container-release; };
        devShell = craneLib.devShell {
          packages = with pkgs; [
            rust-analyzer
            cargo-watch
            trunk
            tailwindcss
            wasm-pack
            sqlx-cli
            sqlite
            capnproto
          ];
          UPDATE_GOLDENFILES = "1";
          # `cargo sqlx prepare` needs an absolute path (`database create` and `migrate run` don't)
          shellHook = ''
            if test -d crates/web-service/data; then
              export DATABASE_URL="sqlite://$(pwd)/crates/web-service/data/logjuicer.sqlite?mode=rwc";
            else
              export DATABASE_URL="sqlite://$(pwd)/data/logjuicer.sqlite?mode=rwc";
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
