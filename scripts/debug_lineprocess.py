#!/usr/bin/env python3
#
# Licensed under the Apache License, Version 2.0 (the "License"); you may
# not use this file except in compliance with the License. You may obtain
# a copy of the License at
#
#      http://www.apache.org/licenses/LICENSE-2.0
#
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS, WITHOUT
# WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied. See the
# License for the specific language governing permissions and limitations
# under the License.

"""Script to debug line tokenization"""

from collections import Counter
import sys

from logreduce.tokenizer import Tokenizer

try:
    path = sys.argv[1]
except IndexError:
    print("usage: %s [file]..." % sys.argv[0])
    exit(1)

tokens_c = Counter()
word_c = Counter()
line_set = set()
for path in sys.argv[1:]:
    for line in open(path):
        word_c.update(line.split())
        tokens = Tokenizer.process(line)
        tokens_c.update(tokens.split())
        line = line.rstrip()
        if line not in line_set and (line != tokens):
            line_set.add(line)
            print("  ", line)
            print("-> %s" % tokens)

print("Total words: %d Total Tokens: %d" % (
        len(word_c), len(tokens_c)))

print("Top 10 words: %s", word_c.most_common(10))
print("Top 10 Tokens: %s", tokens_c.most_common(10))
