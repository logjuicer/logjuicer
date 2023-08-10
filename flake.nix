# Build release with: nix -L build .#release
{
  description = "The logreduce app";

  inputs = {
    # nixpkgs is tracking nixpkgs-unstable
    nixpkgs.url = "github:NixOS/nixpkgs/3d6ebeb283be256f008541ce2b089eb5fb0e4e01";
    naersk.url =
      "github:nix-community/naersk/d9a33d69a9c421d64c8d925428864e93be895dcc";
    naersk.inputs.nixpkgs.follows = "nixpkgs";
    fenix.url =
      "github:nix-community/fenix/6c9f0709358f212766cff5ce79f6e8300ec1eb91";
    fenix.inputs.nixpkgs.follows = "nixpkgs";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = inputs@{ self, nixpkgs, naersk, fenix, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = nixpkgs.legacyPackages.${system};
        naersk-lib = naersk.lib."${system}";
        logreduce = naersk-lib.buildPackage {
          pname = "logreduce";
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

        toolchain = fenix.packages.${system}.default.toolchain;

        # static build
        toolchain-musl = with fenix.packages.${system};
          combine [
            minimal.rustc
            minimal.cargo
            targets.x86_64-unknown-linux-musl.latest.rust-std
          ];
        naersk-musl-lib = naersk.lib.${system}.override {
          cargo = toolchain-musl;
          rustc = toolchain-musl;
        };
        static-exe = naersk-musl-lib.buildPackage {
          pname = "logreduce";
          src = self;

          nativeBuildInputs = with pkgs; [ pkgsStatic.stdenv.cc ];

          CARGO_BUILD_TARGET = "x86_64-unknown-linux-musl";
          CARGO_BUILD_RUSTFLAGS = "-C target-feature=+crt-static";

          # lorgeduce-httpdir test are broken with musl
          doCheck = false;
        };
        release = pkgs.runCommand "logreduce-release" { } ''
          echo Creating release tarball with ${static-exe}
          cd ${static-exe};
          tar -cf - bin/ | ${pkgs.bzip2}/bin/bzip2 -9 > $out
          echo cp $out logreduce-x86_64-linux.tar.bz2
        '';

      in {
        defaultPackage = logreduce;
        apps.default = flake-utils.lib.mkApp { drv = logreduce; };
        devShell = pkgs.mkShell {
          buildInputs = with pkgs; [ toolchain openssl pkg-config ];
          LOGREDUCE_CACHE = "1";
        };

        # nix develop .#python
        packages.python = python;
        devShells.python = pkgs.mkShell { buildInputs = [ python ]; };

        # nix build .#static
        packages.static = static-exe;
        packages.release = release;
      });
}
