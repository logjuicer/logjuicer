# Copyright (C) 2022 Red Hat
# SPDX-License-Identifier: Apache-2.0

"""
This benchmark compares both python and rust runtime.
Though the process is quite different: the new implementation does more work to
provides a better tokenization.
In particular, instead of applying regex replacement on the whole line, the new code
tries to tokenize each words, using a recursive algorithm to break down composite words.
"""

import timeit
import logreduce_rust

# A very simple approximation of the new process
import re
http_re = re.compile("http[^ ]*", re.IGNORECASE)
months_re = re.compile(
    "sunday|monday|tuesday|wednesday|thursday|friday|saturday|"
    "january|february|march|april|may|june|july|august|september|"
    "october|november|december", re.IGNORECASE
)
word_re = re.compile("[ \t]")
def native_process(line):
    result = ""
    for word in word_re.split(line):
        if http_re.match(word):
            result += "URL"
        elif months_re.match(word):
            result += "MONTH"
        else:
            result += word
        result += " "
    return result

# Bench function
data = open("../LICENSE").readlines()
def bench(process):
    return timeit.timeit(lambda: [process(line) for line in data], number=1000) * 1000

py = bench(native_process)
print("Python   {:.0f}ms".format(py))
rs = bench(logreduce_rust.process)
print("Rust     {:.0f}ms ({:.1f} times faster)".format(rs, py / rs))

import logreduce.tokenizer
base = bench(logreduce.tokenizer.Tokenizer.process)
print("Baseline {:.0f}ms".format(base))
