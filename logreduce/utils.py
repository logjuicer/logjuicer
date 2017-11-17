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
    "postci.txt",
    "heat-deploy-times.log.txt",
    "btmp.txt",
    "wtmp.txt",
    "lastlog.txt",
    "host_info.txt",
    "devstack-gate-setup-host.txt",
    "service_configs.json.txt",
    "worlddump-",
    "id_rsa",
    "tempest.log.txt",
    "tempest_output.log.txt",
)
BLACKLIST_EXTENSIONS = (
    ".db",
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
IGNORE_PATH = (
    "group_vars/all.yaml",
    "keystone/credential-keys/1",
    # extra/logstash is already printed in deploy logs
    "extra/logstash.txt",
    "migration/identity.gz",
    "swift/backups/",
)
IGNORE_FILES = [
    "index.html",
]


def open_file(p):
    if p.endswith(".gz"):
        # check if really gzip, logs.openstack.org return decompressed files
        if open(p, 'rb').read(2) == b'\x1f\x8b':
            return gzip.open(p, mode='r')
    return open(p, 'rb')


def files_iterator(paths):
    """Walk directory and yield (path, rel_path)"""
    if not isinstance(paths, list):
        paths = [paths]
    else:
        # Copy path list
        paths = list(paths)
    for path in paths:
        if os.path.isfile(path):
            yield (path, os.path.basename(path))
        elif os.path.isdir(path):
            if path[-1] != "/":
                path = "%s/" % path
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
                    yield (fpath, fpath[len(path):])
        else:
            raise RuntimeError("%s: unknown uri" % path)


def setup_logging(debug=False, name="LogReduce"):
    loglevel = logging.INFO
    if debug:
        loglevel = logging.DEBUG
    logging.basicConfig(
        format='%(asctime)s %(levelname)-5.5s %(name)s - %(message)s',
        level=loglevel)
    return logging.getLogger(name)


def format_speed(count, size, elapsed_time):
    """Return speed in MB/s and kilo-line count/s"""
    return "%.03fs at %.03fMB/s (%0.3fkl/s) (%.03f MB - %.03f kilo-lines)" % (
        elapsed_time,
        (size / (1024 * 1024)) / elapsed_time,
        (count / 1000) / elapsed_time,
        (size / (1024 * 1024)),
        (count / 1000),
    )
