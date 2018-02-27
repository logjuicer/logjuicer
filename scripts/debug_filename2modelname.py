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

"""Script to debug file groups"""

import sys
from logreduce.utils import files_iterator
from logreduce.models import Classifier

try:
    path = sys.argv[1]
except IndexError:
    print("usage: %s dir" % sys.argv[0])
    exit(1)

groups = {}
for filename, filename_rel in files_iterator(path):
    bag_name = Classifier.filename2modelname(filename_rel)
    groups.setdefault(bag_name, []).append(filename)

for group_name, files in sorted(groups.items()):
    print("%s:" % group_name)
    for f in files:
        print("\t%s" % f)
