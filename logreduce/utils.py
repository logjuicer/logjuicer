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
import urllib.request

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
)


DAYS="sunday|monday|tuesday|wednesday|thursday|friday|saturday"
MONTHS="january|february|march|april|may|june|july|august|september|october|" \
       "november|december"
RANDOM_PREFIXES=r'tmp|br|tap|req-|ns-|ansible_|0x|a[0-9]+='
MIXED_ALPHA_DIGITS_WORDS=r'[a-z0-9+]*[0-9][a-z0-9\/+]*'

DEBUG_TOKEN=False


class Tokenizer:
    randword_re = re.compile(r'\b(' +
                             r'%s' % DAYS +
                             r'|%s' % MONTHS +
                             r'|%s' % RANDOM_PREFIXES +
                             r'|%s' % MIXED_ALPHA_DIGITS_WORDS +
                             r')[^\s\/]*', re.I)
    alpha_re = re.compile(r'[^a-zA-Z_\/\s]')

    @staticmethod
    def process(line):
        """Extract interesing part"""
        strip = line
        # Remove known random word
        strip = Tokenizer.randword_re.subn(" ", strip)[0]
        # Only keep characters
        strip = Tokenizer.alpha_re.subn(" ", strip)[0]
        # Remove tiny words
        strip = " ".join(filter(lambda x: len(x) > 3, strip.split()))
        if DEBUG_TOKEN:
            print("[%s] => [%s]" % (line, strip))
        return strip

    @staticmethod
    def filename2modelname(filename):
        """Create a modelname based on filename"""
        # Only keep parent directory and first component of the basename
        # For example: puppet-20170620_063554.txt.gz -> puppet-_.txt
        shortfilename = os.path.join(
            os.path.basename(os.path.dirname(filename)),
            os.path.basename(filename).split('.')[0])
        # Detect jenkins jobs in path
        # For example: jenkins/jobs/config-update/42/log -> config-update/log
        if "/jobs/" in filename:
            job_name = filename.split('/jobs/')[-1].split('/')[0]
            shortfilename = os.path.join(job_name, shortfilename)
        # Append relevant extensions
        for ext in (".conf", ".audit", ".txt", ".yaml", ".orig", ".log",
                    ".xml"):
            if ext in filename:
                shortfilename += ext
        # Remove numbers and symbols
        return re.subn(r'[^a-zA-Z\/\._-]*', '', shortfilename)[0]


def download(url, path):
    """Download console logs to a file"""
    try:
        if not os.path.isdir(os.path.dirname(path)):
            os.makedirs(os.path.dirname(path), 0o755)
        with urllib.request.urlopen(url) as response:
            with open(path, "w") as of:
                of.write(response.read().decode('utf-8'))
    except:
        print("ERROR - Couldn't download %s to %s" % (url, path))
        raise


def files_iterator(paths):
    """Yield (path, file object)"""
    def open_file(p):
        if p.endswith(".gz"):
            return gzip.open(p, mode='rt')
        return open(p)

    if not isinstance(paths, list):
        paths = [paths]
    for path in paths:
        if os.path.isfile(path):
            yield (path, open_file(path))
        elif os.path.isdir(path):
            for dname, _, fnames in os.walk(path):
                for fname in fnames:
                    if [True for skip in BLACKLIST if fname.startswith(skip)]:
                        continue
                    if [True for skip in BLACKLIST_EXTENSIONS if
                            fname.endswith("%s" % skip) or
                            fname.endswith("%s.gz" % skip)]:
                        continue
                    fpath = os.path.join(dname, fname)
                    if "/.git/" in fpath:
                        continue
                    yield (fpath, open_file(fpath))
        else:
            raise RuntimeError("%s: unknown uri" % path)
