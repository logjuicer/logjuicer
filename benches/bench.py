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
word_re = re.compile("[ \t]")
def native_process(line):
    result = ""
    for word in word_re.split(line):
        if len(word) < 4:
            continue
        if http_re.match(word):
            result += "URL"
        elif months_re.match(word):
            result += "MONTH"
        else:
            result += word
    return result

# Bench function
data = open("LICENSE").readlines()
def bench(process):
    return timeit.timeit(lambda: [process(line) for line in data], number=1000) * 1000

py = bench(native_process)
print("Python   {:.0f}ms".format(py))
rs = bench(logreduce_tokenizer.process)
print("Rust     {:.0f}ms ({:.1f} times faster)".format(rs, py / rs))

import tokenizer
base = bench(tokenizer.Tokenizer.process)
print("Baseline {:.0f}ms".format(base))
