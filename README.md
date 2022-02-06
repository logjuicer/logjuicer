# logreduce-tokenizer

Build:

```
python setup.py build
export PYTHONPATH=$(pwd)/build/lib
```

Bench:

```
python benches/bench.py
```

Test perf:

```
RUSTFLAGS="-C target-cpu=native" cargo build --release
```

Build CLI:

```
RUSTFLAGS="-C target-cpu=native" cargo build --example logreduce-tokenizer-cli --release
```
