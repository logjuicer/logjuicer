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
import subprocess
import time
import urllib.request
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
)
IGNORE_FILES = [
    "*.rpm",
    "index.html",
]


DAYS="sunday|monday|tuesday|wednesday|thursday|friday|saturday"
MONTHS="january|february|march|april|may|june|july|august|september|october|" \
       "november|december"
RANDOM_PREFIXES=r'tmp|br|tap|req-|ns-|ansible_|dib_build\.|0x|a[0-9]+='
MIXED_ALPHA_DIGITS_WORDS=r'[a-z0-9+]*[0-9][a-z0-9\/+]*'

DEBUG_TOKEN=False
UPDATE_CACHE=False


class Tokenizer:
    randword_re = re.compile(r'\b(' +
                             r'%s' % DAYS +
                             r'|%s' % MONTHS +
                             r'|%s' % RANDOM_PREFIXES +
                             r'|%s' % MIXED_ALPHA_DIGITS_WORDS +
                             r')[^\s\/]*', re.I)
    comments = re.compile(r'[\s]*# .*')
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
        shortfilename = Tokenizer.randword_re.subn("", shortfilename)[0]
        # Detect jenkins jobs in path
        # For example: jenkins/jobs/config-update/42/log -> config-update/log
        if "/jobs/" in filename:
            job_name = filename.split('/jobs/')[-1].split('/')[0]
            shortfilename = os.path.join(job_name, shortfilename)
        # Detect results in path
        if "/results/" in filename:
            job_name = filename.split('results/')[-1].split('/')[0]
            shortfilename = os.path.join(job_name, shortfilename)
        if shortfilename == '':
            # Reduction was too agressive, just keep the filename in this case
            shortfilename = os.path.basename(filename).split('.')[0]
        # Append relevant extensions
        for ext in (".conf", ".audit", ".txt", ".yaml", ".orig", ".log",
                    ".xml"):
            if ext in filename:
                shortfilename += ext
        # Remove numbers and symbols
        return re.subn(r'[^a-zA-Z\/\._-]*', '', shortfilename)[0]


def download(url, expiry=None):
    """Download helper"""
    local_path = "%s/%s" % (CACHE, url.replace(
        'https://', '').replace('http://', ''))
    if not os.path.isdir(os.path.dirname(local_path.rstrip('/'))):
        os.makedirs(os.path.dirname(local_path.rstrip('/')), 0o755)
    if url[-1] == "/":
        if not os.path.isdir(local_path) or not os.listdir(local_path) or UPDATE_CACHE:
            cmd = ["lftp", "-c", "mirror", "-c"]
            for ign in IGNORE_FILES:
                cmd.extend(["-X", ign])
            cmd.extend([
                "-X", "index.html", "-X", "*.rpm",
                "-x", "ara", "-x", "ara-report", "-x", "_zuul_ansible",
                url, local_path
            ])
            log.debug("Running %s" % " ".join(cmd))
            if subprocess.Popen(cmd).wait():
                raise RuntimeError("%s: Couldn't mirror" % url)
    else:
        if not os.path.isfile(local_path) or \
           os.stat(local_path).st_size == 0 or \
           (expiry and (time.time() - os.stat(local_path).st_mtime) > expiry) or \
           UPDATE_CACHE:
            if not os.path.isdir(os.path.dirname(local_path)):
                os.makedirs(os.path.dirname(local_path), 0o755)
            try:
                with urllib.request.urlopen(url) as response:
                    data = response.read().decode('utf-8')
                # Make sure it wasn't an index of page
                if "<title>index of" in data[:1024].lower():
                    log.info("%s is an index of, mirroring with lftp" % url)
                    return download("%s/" % url)
                with open(local_path, "w") as of:
                    of.write(data)

            except:
                print("ERROR - Couldn't download %s to %s" % (url, local_path))
                raise
    return local_path


def open_file(p):
    if p.endswith(".gz"):
        # check if really gzip, logs.openstack.org return decompressed files
        if open(p, 'rb').read(2) == b'\x1f\x8b':
            return gzip.open(p, mode='rt')
    return open(p)


def files_iterator(paths):
    """Yield (path, original uri, file object)"""
    if not isinstance(paths, list):
        paths = [paths]
    else:
        # Copy path list
        paths = list(paths)
    last_url = None
    for path in paths:
        if os.path.isfile(path):
            yield (path, path, open_file(path))
        elif path.startswith("http://") or path.startswith("https://"):
            local_path = download(path)
            if path[-1] == "/":
                last_url = path
                paths.append(local_path)
            else:
                yield (local_path, path, open_file(local_path))
        elif os.path.isdir(path):
            for dname, _, fnames in os.walk(path):
                for fname in fnames:
                    if [True for ign in IGNORE_FILES if fname == ign]:
                        continue
                    if [True for skip in BLACKLIST if fname.startswith(skip)]:
                        continue
                    if [True for skip in BLACKLIST_EXTENSIONS if
                            fname.endswith("%s" % skip) or
                            fname.endswith("%s.gz" % skip)]:
                        continue
                    fpath = os.path.join(dname, fname)
                    if "/.git/" in fpath:
                        continue
                    if last_url:
                        if last_url[-1] == "/":
                            # Remove local directory tree
                            rel_path = fpath[len(local_path):]
                            path_orig = "%s%s" % (last_url, rel_path)
                        else:
                            path_orig = last_url
                    else:
                        path_orig = fpath
                    yield (fpath, path_orig, open_file(fpath))
            last_url = None
        else:
            raise RuntimeError("%s: unknown uri" % path)
