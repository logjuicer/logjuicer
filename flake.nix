{
  description = "The logreduce-cli app";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    naersk.url = "github:nix-community/naersk";
    naersk.inputs.nixpkgs.follows = "nixpkgs";
    fenix.url = "github:nix-community/fenix";
    fenix.inputs.nixpkgs.follows = "nixpkgs";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, naersk, fenix, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = nixpkgs.legacyPackages.${system};
        naersk-lib = naersk.lib."${system}";
        logreduce = naersk-lib.buildPackage {
          pname = "logreduce-cli";
          src = self;
          nativeBuildInputs = with pkgs; [ openssl pkg-config ];
          doCheck = true;
        };

        # python toolchain
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

        # static build
        toolchain = with fenix.packages.${system};
          combine [
            minimal.rustc
            minimal.cargo
            targets.x86_64-unknown-linux-musl.latest.rust-std
          ];
        naersk-musl-lib = naersk.lib.${system}.override {
          cargo = toolchain;
          rustc = toolchain;
        };
      in {
        defaultPackage = logreduce;
        apps.default = flake-utils.lib.mkApp { drv = logreduce; };
        devShell = pkgs.mkShell {
          buildInputs = with pkgs; [ rustc cargo clippy rustfmt openssl pkg-config ];
          LOGREDUCE_CACHE = "1";
        };

        # nix develop .#python
        packages.python = python;
        devShells.python = pkgs.mkShell { buildInputs = [ python ]; };

        # nix build .#static
        packages.static = naersk-musl-lib.buildPackage {
          pname = "logreduce-cli";
          src = self;

          nativeBuildInputs = with pkgs; [ pkgsStatic.stdenv.cc ];

          CARGO_BUILD_TARGET = "x86_64-unknown-linux-musl";
          CARGO_BUILD_RUSTFLAGS = "-C target-feature=+crt-static";

          # lorgeduce-httpdir test are broken with musl
          doCheck = true;
        };
      });
}
