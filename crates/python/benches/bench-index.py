# Copyright (C) 2022 Red Hat
# SPDX-License-Identifier: Apache-2.0

import timeit
import logreduce_rust
import logreduce.models

# Bench function
data = logreduce_rust.generate(20_000).split('\n')
train_data = data[:-512]
test_data = data[-512:]

def python():
    model = logreduce.models.HashingNeighbors()
    model.train(train_data)
    model.test(test_data)

def rust():
    model = logreduce_rust.index_mat(train_data)
    logreduce_rust.search_mat(model, test_data)

def bench(process):
    return timeit.timeit(lambda: process(), number=10) * 100

py = bench(python)
print("Python {:>6.1f}ms".format(py))
rs = bench(rust)
print("Rust   {:>6.1f}ms ({:.1f} times faster)".format(rs, py / rs))
