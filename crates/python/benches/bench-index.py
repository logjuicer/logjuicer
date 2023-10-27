# Copyright (C) 2022 Red Hat
# SPDX-License-Identifier: Apache-2.0

import timeit
import logjuicer_rust
import logjuicer.models

# Bench function
data = logjuicer_rust.generate(20_000).split('\n')
train_data = data[:-512]
test_data = data[-512:]

def python():
    model = logjuicer.models.HashingNeighbors()
    model.train(train_data)
    model.test(test_data)

def rust():
    model = logjuicer_rust.index_mat(train_data)
    logjuicer_rust.search_mat(model, test_data)

def bench(process):
    return timeit.timeit(lambda: process(), number=10) * 100

py = bench(python)
print("Python {:>6.1f}ms".format(py))
rs = bench(rust)
print("Rust   {:>6.1f}ms ({:.1f} times faster)".format(rs, py / rs))
