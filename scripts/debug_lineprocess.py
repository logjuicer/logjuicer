#!/bin/env python3
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

import sys
from logreduce.tokenizer import Tokenizer

try:
    path = sys.argv[1]
except IndexError:
    print("usage: %s file" % sys.argv[0])
    exit(1)

for line in open(path).readlines():
    print(line[:-1])
    print("-> %s" % Tokenizer.process(line))
