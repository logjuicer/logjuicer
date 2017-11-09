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

import gzip
import os
import re
import logging


log = logging.getLogger("logreduce.utils")

CACHE = "/tmp/logs-cache"

# Avoid those files that aren't useful for words analysis
BLACKLIST = (
    "lsof_network.txt",
    "uname.txt",
    "sysstat.txt",
    "df.txt",
    "rdo-trunk-deps-end.txt",
    "meminfo.txt",
    "repolist.txt",
    "hosts.txt",
    "lsof.txt",
    "lsmod.txt",
    "sysctl.txt",
    "cpuinfo.txt",
    "pstree.txt",
    "iotop.txt",
    "iostat.txt",
    "free.txt",
    "dstat.txt",
)
BLACKLIST_EXTENSIONS = (
    ".ico",
    ".png",
    ".tgz",
    ".pyc",
    ".pyo",
    ".so",
    ".key",
    "_key",
    ".crt",
    ".pem",
    ".rpm",
)
IGNORE_FILES = [
    "index.html",
]
IGNORE_PATH = [
    "group_vars/all.yaml"
]


DAYS = "sunday|monday|tuesday|wednesday|thursday|friday|saturday"
MONTHS = "january|february|march|april|may|june|july|august|september|" \
         "october|november|december"
RANDOM_PREFIXES = r'tmp|br|tap|req-|ns-|0x|a[0-9]+='
RANDOM_DIRS = r'ansible_|omit_place_holder__|instack\.|dib_build\.'
MIXED_ALPHA_DIGITS_WORDS = r'[a-z0-9+]*[0-9][a-z0-9\/+]*'


class Tokenizer:
    randword_re = re.compile(r'\b(' +
                             r'%s' % DAYS +
                             r'|%s' % MONTHS +
                             r'|%s' % RANDOM_PREFIXES +
                             r'|%s' % RANDOM_DIRS +
                             r'|%s' % MIXED_ALPHA_DIGITS_WORDS +
                             r')[^\s\/]*', re.I)
    comments = re.compile(r'([\s]*# |^%% |^#|^[\s]*id = ").*')
    alpha_re = re.compile(r'[^a-zA-Z_\/\s]')

    @staticmethod
    def process(line):
        """Extract interesing part"""
        strip = line
        # Remove comments
        strip = Tokenizer.comments.subn(" ", strip)[0]
        # Remove known random word
        strip = Tokenizer.randword_re.subn(" ", strip)[0]
        # Only keep characters
        strip = Tokenizer.alpha_re.subn(" ", strip)[0]
        # Remove tiny words
        strip = " ".join(filter(lambda x: len(x) > 3, strip.split()))
        return strip

    @staticmethod
    def filename2modelname(filename):
        """Create a modelname based on filename"""
        # Only keep parent directory and first component of the basename
        shortfilename = os.path.join(
            Tokenizer.randword_re.subn("", os.path.basename(
                os.path.dirname(filename)))[0],
            os.path.basename(filename).split('.')[0])
        # Detect pipeline in path and add job name
        for pipeline in ("check", "gate", "post", "periodic"):
            pipedir = "/%s/" % pipeline
            if pipedir in filename:
                job_name = filename.split(pipedir)[-1].split('/')[0]
                shortfilename = os.path.join(job_name, shortfilename)
        if shortfilename == '':
            # Reduction was too agressive, just keep the filename in this case
            shortfilename = os.path.basename(filename).split('.')[0]
        # Append relevant extensions
        for ext in (".conf", ".audit", ".yaml", ".orig", ".log",
                    ".xml", ".html", ".txt", ".py", ".json", ".yml"):
            if filename.endswith(ext) or filename.endswith("%s.gz" % ext):
                shortfilename += ext
        # Remove numbers and symbols
        return re.subn(r'[^a-zA-Z\/\._-]*', '', shortfilename)[0]


def open_file(p):
    if p.endswith(".gz"):
        # check if really gzip, logs.openstack.org return decompressed files
        if open(p, 'rb').read(2) == b'\x1f\x8b':
            return gzip.open(p, mode='rt')
    return open(p)


def files_iterator(paths):
    """Walk directory and yield path"""
    if not isinstance(paths, list):
        paths = [paths]
    else:
        # Copy path list
        paths = list(paths)
    for path in paths:
        if os.path.isfile(path):
            yield path
        elif os.path.isdir(path):
            for dname, _, fnames in os.walk(path):
                for fname in fnames:
                    if [True for ign in IGNORE_FILES if re.match(ign, fname)]:
                        continue
                    if [True for skip in BLACKLIST if fname.startswith(skip)]:
                        continue
                    if [True for skip in BLACKLIST_EXTENSIONS if
                            fname.endswith("%s" % skip) or
                            fname.endswith("%s.gz" % skip)]:
                        continue
                    fpath = os.path.join(dname, fname)
                    if [True for ign in IGNORE_PATH if re.search(ign, fpath)]:
                        continue
                    if "/.git/" in fpath:
                        continue
                    yield fpath
        else:
            raise RuntimeError("%s: unknown uri" % path)
