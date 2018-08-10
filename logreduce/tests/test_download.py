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

import unittest
import json
import uuid
from mock import patch

import logreduce.download


class MockResponse(object):
    def __init__(self, resp_data, code=200, msg='OK'):
        self.resp_data = resp_data
        self.status = code
        self.msg = msg
        self.headers = {'content-type': 'text/plain; charset=utf-8'}

    def read(self):
        return self.resp_data.encode('utf-8')


class DownloadTests(unittest.TestCase):
    @patch('urllib.request.urlopen')
    def test_zuul_builds(self, mock_request):
        fake_builds = []
        for i in range(3):
            build_uuid = str(uuid.uuid4())
            fake_builds.append({
                "uuid": build_uuid,
                "branch": "master",
                "results": "SUCCESS",
                "ref_url": "http://zuul.example.com/change/42",
                "log_url": "http://zuul.example.com/logs/%s" % build_uuid,
            })
        mock_request.return_value = MockResponse(json.dumps(fake_builds))
        zb = logreduce.download.ZuulBuilds("http://zuul.example.com/api")
        self.assertEquals(3, len(zb.get(result="SUCCESS")))
