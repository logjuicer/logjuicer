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

import io
import unittest
import os

import logreduce.process


class ProcessTests(unittest.TestCase):
    def test_process_diff(self):
        # Compare two python test files
        clf = logreduce.process.Classifier()
        baseline = __file__
        target = os.path.join(os.path.dirname(baseline), "test_download.py")
        clf.train(baseline)
        for file_result in clf.test(target):
            filename, filename_orig, model, outliers, test_time = file_result
            assert os.path.basename(model.sources[0]) == "test_process.py"
            assert filename == "test_download.py"
            assert test_time > 0
            assert len(outliers) > 0
            assert isinstance(outliers[0][0], int), 'line number wrong type'
            assert isinstance(outliers[0][1], float), 'distance wrong type'
            assert isinstance(outliers[0][2], str), 'line wrong type'
            assert outliers[0][0] > 0, 'license matched as anomaly'

        # Save model and reload the model
        model = io.BytesIO()
        model.name = ":memory:"
        clf.save(model)
        model.seek(0)
        logreduce.process.Classifier.check(model)
        # joblib load reset the seek for io bytes, bypass model check in test
        model = io.BytesIO(model.read())
        import sklearn
        clf = sklearn.externals.joblib.load(model)

        # Re-use the model with another test file
        target = os.path.join(os.path.dirname(baseline), "test_units.py")
        for file_result in clf.test(target):
            filename, filename_orig, model, outliers, test_time = file_result
            assert os.path.basename(model.sources[0]) == "test_process.py"
            assert filename == "test_units.py"
            assert test_time > 0
            assert len(outliers) > 0
            assert isinstance(outliers[0][0], int), 'line number wrong type'
            assert isinstance(outliers[0][1], float), 'distance wrong type'
            assert isinstance(outliers[0][2], str), 'line wrong type'
            assert outliers[0][0] > 0, 'license matched as anomaly'

        # Test the process method
        result = clf.process(target)
        assert result['baselines'] == [__file__]
        assert result['targets'] == [target]
        assert 'test_units.py' in result['files']
        file_info = result['files']['test_units.py']
        assert result['models']['test_process.py'].get('uuid') != ''
        assert file_info['mean_distance'] > 0.0
        assert file_info['mean_distance'] < 1.0
        assert isinstance(file_info['lines'][0], str), 'line wrong type'
        scores = file_info['scores']
        assert isinstance(scores[0][0], int), 'line number wrong type'
        assert isinstance(scores[0][1], float), 'distance wrong type'
        assert scores[0][0] > 0, 'license matched as anomaly'
