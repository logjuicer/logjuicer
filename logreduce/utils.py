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
import lzma
import os
import re
import logging

# Avoid those files that aren't useful for words analysis
DEFAULT_IGNORE_PATHS = [
    "zuul-info/",
    # sf-ci useless static files
    "/executor.*/trusted/",
    # tripleo-ci static files
    "/etc/selinux/targeted/",
    "/etc/sysconfig/",
    "/etc/systemd/",
    "/etc/polkit-1/",
    "/etc/pki/",
    "group_vars/all.yaml",
    "keystone/credential-keys/1",
    # extra/logstash is already printed in deploy logs
    "extra/logstash.txt",
    "migration/identity.gz",
    "swift/backups/",
    "/\.git/",
    "/\.svn/",
]

DEFAULT_IGNORE_FILES = [
    '_zuul_ansible/',
    'ara-report/',
    'ara-sf/',
    'ara/',
    'btmp.txt',
    'cpuinfo.txt',
    'devstack-gate-setup-host.txt',
    'df.txt',
    'dstat.txt',
    'free.txt',
    'heat-deploy-times.log.txt',
    'host_info.txt',
    'hosts.txt',
    'id_rsa',
    'index.html',
    'iostat.txt',
    'iotop.txt',
    'lastlog.txt',
    'lsmod.txt',
    'lsof.txt',
    'lsof_network.txt',
    'meminfo.txt',
    'nose_results.html',
    'passwords.yml',
    'postci.txt',
    'pstree.txt',
    'rdo-trunk-deps-end.txt',
    'repolist.txt',
    'service_configs.json.txt',
    'sysctl.txt',
    'sysstat.txt',
    'tempest.log.txt',
    'tempest_output.log.txt',
    'uname.txt',
    'worlddump-',
    'wtmp.txt',
]

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
    ".subunit",
    ".journal",
    ".json",
    ".conf",
)


def open_file(p):
    if p.endswith(".gz"):
        # check if really gzip, logs.openstack.org return decompressed files
        if open(p, 'rb').read(2) == b'\x1f\x8b':
            return gzip.open(p, mode='r')
    elif p.endswith(".xz"):
        return lzma.open(p, mode='r')
    return open(p, 'rb')


def files_iterator(paths, ign_files=[], ign_paths=[]):
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
                    if [True for ign in ign_files if re.match(ign, fname)]:
                        continue
                    if [True for skip in BLACKLIST_EXTENSIONS if
                            fname.endswith("%s" % skip) or
                            fname.endswith("%s.gz" % skip) or
                            fname.endswith("%s.bz2" % skip) or
                            fname.endswith("%s.xz" % skip)]:
                        continue
                    fpath = os.path.join(dname, fname)
                    rel_path = fpath[len(path):]
                    if [True for ign in ign_paths if re.search(ign, rel_path)]:
                        continue
                    yield (fpath, rel_path)
        else:
            raise RuntimeError("%s: unknown uri" % path)


def setup_logging(debug=False):
    loglevel = logging.INFO
    if debug:
        loglevel = logging.DEBUG
    logging.basicConfig(
        format='%(asctime)s %(levelname)-5.5s %(name)s - %(message)s',
        level=loglevel)


def format_speed(count, size, elapsed_time):
    """Return speed in MB/s and kilo-line count/s"""
    return "%.03fs at %.03fMB/s (%0.3fkl/s) (%.03f MB - %.03f kilo-lines)" % (
        elapsed_time,
        (size / (1024 * 1024)) / elapsed_time,
        (count / 1000) / elapsed_time,
        (size / (1024 * 1024)),
        (count / 1000),
    )
