{
  description = "The logreduce-cli app";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/release-22.11";
    naersk.url =
      "github:nix-community/naersk/88cd22380154a2c36799fe8098888f0f59861a15";
    naersk.inputs.nixpkgs.follows = "nixpkgs";
    fenix.url =
      "github:nix-community/fenix/2914d6b361c565356da6c03a8b36bc240f188aef";
    fenix.inputs.nixpkgs.follows = "nixpkgs";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = inputs@{ self, nixpkgs, naersk, fenix, flake-utils }:
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
        toolchain = fenix.packages.${system}.default.toolchain;
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

      });
}
