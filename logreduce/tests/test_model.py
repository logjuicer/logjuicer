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

import logreduce.server.model as model
import logreduce.server.client

from . utils import fake_build_result


class DBTests(unittest.TestCase):
    def setUp(self):
        self.db = model.Db("sqlite://")

    def _add_model(self, name, build_count=2):
        baselines = []
        for buildid in range(build_count):
            build = model.Build(
                uuid="2%s" % buildid,
                pipeline="periodic",
                log_url="http://logs/%d/" % buildid)
            baseline = model.Baseline(path=name, build=build)
            baselines.append(baseline)
        return model.Model(
            uuid="2%s" % buildid, name=name, baselines=baselines)

    def test_model_logfile(self):
        """Test logfile <-> model <-> baseline <-> build relationships"""
        lid = None
        with self.db.session() as session:
            # First register a model
            m = self._add_model("message")
            logfile = model.LogFile(path="message", model=m)
            session.add(logfile)
            session.commit()
            lid = logfile.id

            logfile = session.query(model.LogFile).get(lid)
            assert "message" == logfile.path
            assert "message" == logfile.model.name
            assert "message" == logfile.model.baselines[0].path
            assert "periodic" == logfile.model.baselines[0].build.pipeline

    def test_model_anomaly(self):
        """Test a complete anomaly record"""
        with self.db.session() as session:
            # Create a model and logfile
            m = self._add_model("message")
            logfile = model.LogFile(path="message", model=m, lines=[
                model.Line(nr=1, confidence=0.0),
                model.Line(nr=2, confidence=0.5)
            ])
            # Register the target build and the anomaly
            a = model.Anomaly(uuid="test", name="test-anomaly",
                              build=model.Build(uuid="42", pipeline="check"),
                              logfiles=[logfile])
            session.add(a)
            session.commit()

            anomaly = session.query(model.Anomaly).get("test")
            assert "test-anomaly" == anomaly.name
            assert "check" == anomaly.build.pipeline
            baselines = anomaly.logfiles[0].model.baselines
            assert "periodic" == baselines[0].build.pipeline

    def test_import_report(self):
        report = logreduce.server.client.prepare_report(fake_build_result)
        with self.db.session() as session:
            anomaly_uuid = self.db.import_report(session, report)

            anomaly = session.query(model.Anomaly).get(anomaly_uuid)
            self.assertEquals("check", anomaly.build.pipeline)
