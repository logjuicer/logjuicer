# logreduce-tokenizer

Build:

```
python setup.py build
export PYTHONPATH=$(pwd)/build/lib
```

Bench:

```
python bench.py
```

Test perf:

```
RUSTFLAGS="-C target-cpu=native" cargo build --release
```