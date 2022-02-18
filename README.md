# logreduce-rust

This repository contains packages to implement the [logreduce][logreduce]'s algorithm:

## Libraries

- Tokenizer: [![crates.io](https://img.shields.io/crates/v/logreduce-tokenizer.svg)](https://crates.io/crates/logreduce-tokenizer)

Run tests:

```
cargo test
```

## CLI

Build:

```
RUSTFLAGS="-C target-cpu=native" cargo build --release -p logreduce-cli
```

Use:

```
./target/release/logreduce-cli --help
```

## Python bindings

Build:

```
cd python
python setup.py build
```

Use:

```
export PYTHONPATH=$(pwd)/build/lib
python benches/bench.py
```

[logreduce]: https://github.com/logreduce/logreduce
