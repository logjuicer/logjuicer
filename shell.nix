let
  rust_overlay = (import (builtins.fetchTarball
    "https://github.com/oxalica/rust-overlay/archive/a21c163919cd90dd67bcd03345fac5441e53cccc.tar.gz"));
  pkgs = import (fetchTarball
    "https://github.com/NixOS/nixpkgs/archive/5cf5cad0da6244da30be1b6da2ff3d44b6f3ebe5.tar.gz") {
      overlays = [ rust_overlay ];
    };

  # dependencies of the current python implementation
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
  # dependencies for the new implementation
  rust = [ pkgs.rust-bin.stable.latest.default pkgs.openssl pkgs.pkg-config ];
in pkgs.mkShell { buildInputs = [ python ] ++ rust; }
