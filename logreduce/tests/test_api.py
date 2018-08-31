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

import copy
import json
import logging
import os
import tempfile

from cherrypy.test import helper

import logreduce.server.api as api
import logreduce.server.client
import logreduce.server.rpc as rpc

from . utils import fake_build_result

logging.basicConfig(level=logging.DEBUG)


class APITest(helper.CPWebCase):
    @classmethod
    def setup_class(cls):
        cls.tmpfile = tempfile.mkstemp()[1]
        cls.gearman = {'addr': '0.0.0.0', 'port': 4742}
        cls.gear = rpc.Server(**cls.gearman)
        cls.gear.start()
        cls.downloadLog = []
        super().setup_class()

    @classmethod
    def teardown_class(cls):
        super().teardown_class()
        os.unlink(cls.tmpfile)
        cls.gear.stop()

    def setup_server():
        def fake_handle_download_log(_, url, dest):
            APITest.downloadLog.append((url, dest))
        api.ServerWorker.handle_download_log = fake_handle_download_log
        srv = api.Server(dburi="sqlite:///%s" % APITest.tmpfile, tests=True,
                         gearman=APITest.gearman)
        srv.api.rpc.start()
    setup_server = staticmethod(setup_server)

    def postData(self, path, data=None, method='POST'):
        if data:
            body = json.dumps(data)
            headers = [
                ('Content-Type', 'application/json'),
                ('Content-Length', str(len(body)))
            ]
        else:
            body = None
            headers = None

        self.getPage(
            path,
            headers=headers,
            method=method,
            body=body)

    def getData(self, path):
        self.getPage(path)
        data = self.body.decode('utf-8')
        return json.loads(data)

    def test_api_import_report(self):
        res = self.getData('/api/status')
        self.assertStatus('200 OK')
        assert "functions" in res

        report = logreduce.server.client.prepare_report(fake_build_result)
        self.postData('/api/anomaly', data=report, method='PUT')
        self.assertStatus('200 OK')
        res = json.loads(self.body.decode('utf-8'))
        assert 'uuid' in res
        anomaly_uuid = res['uuid']

        # List
        res = self.getData('/api/anomalies')
        self.assertStatus('200 OK')
        assert anomaly_uuid == res[0]['uuid']
        assert 'processed' == res[0]['status']

        # Get
        res = self.getData('/api/anomaly/' + anomaly_uuid)
        self.assertStatus('200 OK')
        assert 'check' == res['build']['pipeline']
        assert 2 == len(res["models"])

        # Remove a file
        logfile_id = res['logfiles'][0]['id']
        self.postData(
            '/api/anomaly/' + anomaly_uuid + '/logfile/' + str(logfile_id),
            method='DELETE')
        self.assertStatus('200 OK')

        # Check the file got deleted
        res = self.getData('/api/anomaly/' + anomaly_uuid)
        self.assertStatus('200 OK')
        assert logfile_id != res['logfiles'][0]['id']
        assert 1 == len(res["models"])

        # Update scores
        logfile_id = res['logfiles'][0]['id']
        original_scores = res['logfiles'][0]['scores']
        scores = copy.deepcopy(original_scores[2:])
        scores[-1][1] = 0.99
        self.postData(
            '/api/anomaly/' + anomaly_uuid + '/logfile/' + str(logfile_id),
            data=scores,
            method='POST')
        self.assertStatus('200 OK')

        # Check score got updated
        res = self.getData('/api/anomaly/' + anomaly_uuid)
        self.assertStatus('200 OK')
        assert original_scores != res['logfiles'][0]['scores']
        assert 0.99 == res['logfiles'][0]['scores'][-1][1]

        # Check download job was triggered
        assert 0 < len(self.downloadLog)
