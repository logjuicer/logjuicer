# Copyright (C) 2022 Red Hat
# SPDX-License-Identifier: Apache-2.0

import timeit
import logreduce_tokenizer

# Python implementation
import re
http_re = re.compile("http[^ ]*")
months_re = re.compile(
    "january|february|march|april|may|june|july|august|september|"
    "october|november|december"
)
def native_process(line):
    line = line.lower()
    line = http_re.sub("URL", line)
    line = months_re.sub("MONTH", line)
    return line

# Bench function
data = open("LICENSE").readlines()
def bench(process):
    return timeit.timeit(lambda: [process(line) for line in data], number=1000) * 1000

py = bench(native_process)
rs = bench(logreduce_tokenizer.process)

print("Python {:.0f}ms".format(py))
print("Rust   {:.0f}ms ({:.1f} times faster)".format(rs, py / rs))
