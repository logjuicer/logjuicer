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

import json
import logging
import os
import subprocess

from logreduce.utils import download
from logreduce.utils import CACHE


class Jenkins:
    log = logging.getLogger("Jenkins")

    def __init__(self, url, artifacts):
        self.url = url
        self.artifacts = artifacts

    def _job_info(self, job_name):
        fpath = "%s/%s/json" % (CACHE, job_name)
        if not os.path.exists(fpath):
            download("%s/job/%s/api/json" % (self.url, job_name), fpath)
        return json.load(open(fpath))

    def get_last_success_nr(self, job_name):
        return self._job_info(job_name)['lastSuccessfulBuild']['number']

    def get_last_failed_nr(self, job_name):
        return self._job_info(job_name)['lastFailedBuild']['number']

    def get_logs(self, job_name, job_nr):
        url = "%s/job/%s/%s/consoleText" % (self.url, job_name, job_nr)
        fpath = "%s/%s/%s/console" % (CACHE, job_name, job_nr)
        self.log.info("Using %s (cached in %s)" % (url, fpath))
        if not os.path.exists(fpath):
            download(url, fpath)
        if not self.artifacts:
            return fpath
        # Check for artifacts
        dpath = "%s/%s/%s/artifacts" % (CACHE, job_name, job_nr)
        if os.path.isdir(dpath) and os.listdir(dpath):
            return os.path.dirname(dpath)
        artifact_url = open(fpath).readlines()[-2][:-1]
        if artifact_url.startswith("http") and "artifact" in artifact_url:
            cmd = [
                "lftp", "-c", "mirror", "-c",
                "-X", "*.html", "-X", "*.rpm",
                "-x", "ara", "-x", "ara-report",
                artifact_url, "%s/" % dpath
            ]
            self.log.debug("Download artifacts using %s" % cmd)
            subprocess.Popen(cmd).wait()
            return os.path.dirname(dpath)
        return fpath
