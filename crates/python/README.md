# Python bindings

This library provides a module that can be called from Python.

Build:

```ShellSession
python setup.py build --build-lib=build/lib
```

Demo:

```ShellSession
export PYTHONPATH=$(pwd)/build/lib
python benches/bench-index.py
python benches/bench-tokenizer.py
```
