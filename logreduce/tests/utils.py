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

fake_result = {
    'anomalies_count': 18,
    'baselines': ['test_process.py'],
    'files': {
        'test_units.py': {
            'file_url': 'test_units.py',
            'lines': [
                'This is an anomaly...',
            ],
            'scores': [
                (1, 0.8),
            ],
            'mean_distance': 0.8,
            'model': 'test_process.py',
            'test_time': 0.005851114000506641
        }
    },
    'models': {
        'test_process.py': {
            'info': '65 samples, 108 features',
            'source_files': ['test_process.py'],
            'train_time': 0.012661808999837376
        }
    },
    'outlier_lines_count': 1,
    'reduction': 61.76470588235294,
    'targets': ['test_units.py'],
    'testing_lines_count': 34,
    'training_lines_count': 74,
    'total_time': 42,
    'unknown_files': [],
    "test_command": "logreduce dir test_process.py test_units.py"
}
