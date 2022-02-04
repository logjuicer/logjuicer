{ pkgs ? import (fetchTarball
  "https://github.com/NixOS/nixpkgs/archive/5cf5cad0da6244da30be1b6da2ff3d44b6f3ebe5.tar.gz")
  { } }:

let
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
    ]);
in pkgs.mkShell { buildInputs = [ python pkgs.blas pkgs.cargo pkgs.rustc ]; }
