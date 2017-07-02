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
import time

from logreduce.utils import download
from logreduce.utils import CACHE


class Jenkins:
    log = logging.getLogger("Jenkins")

    def __init__(self, url, artifacts):
        self.url = url
        self.artifacts = artifacts

    def _job_info(self, job_name):
        fpath = "%s/%s/json" % (CACHE, job_name)
        if not os.path.exists(fpath) or (time.time() - os.stat(fpath).m_time) > 21600:
            download("%s/job/%s/api/json" % (self.url, job_name), fpath)
        return json.load(open(fpath))

    def get_last_success_nr(self, job_name):
        return self._job_info(job_name)['lastSuccessfulBuild']['number']

    def get_last_failed_nr(self, job_name):
        return self._job_info(job_name)['lastFailedBuild']['number']

    def get_logs(self, job_name, job_nr):
        url = "%s/job/%s/%s/consoleText" % (self.url, job_name, job_nr)
        logs = [url]
        if self.artifacts:
            # Check for artifacts
            local_path = download(url)
            artifacts_url = open(local_path).readlines()[-2][:-1]
            if artifacts_url.startswith("http") and "artifact" in artifacts_url:
                logs.append(artifacts_url)
        return logs
