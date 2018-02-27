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

"""Script to debug binsize"""

import sys
from logreduce.utils import files_iterator, open_file
from logreduce.models import Classifier, Model
from logreduce.models import remove_ansible_std_lines_lists

try:
    path = sys.argv[1]
except IndexError:
    print("usage: %s dir" % sys.argv[0])
    exit(1)

binsize = {}

groups = {}
for filename, filename_rel in files_iterator(path):
    bag_name = Classifier.filename2modelname(filename_rel)
    groups.setdefault(bag_name, []).append(filename)

model = Model()
for group_name, files in sorted(groups.items()):
    for filename in files:
        fobj = None
        try:
            fobj = open_file(filename)
            idx = 0
            while True:
                line = fobj.readline()
                if line == b'':
                    break
                line = line.decode('ascii', errors='ignore')
                # Remove ansible std_lines list now
                line = remove_ansible_std_lines_lists(line)
                for sub_line in line.split(r'\r'):
                    sub_line = model.process_line(sub_line)
                    if sub_line:
                        binsz = len(sub_line.split())
                        if binsz not in binsize:
                            binsize[binsz] = 1
                        else:
                            binsize[binsz] += 1
                idx += 1
        except Exception:
            print("Ooops")
            raise

print(binsize)
for b, c in sorted(binsize.items()):
    print("%d: %d" % (b, c))
