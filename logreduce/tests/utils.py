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
            'train_time': 0.012661808999837376,
            'uuid': '42'
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

fake_build_result = {
    "files": {
        "job-output.txt.gz": {
            # File_url could be reconstructed using the new target value
            "file_url": "job-output.txt.gz",
            "test_time": 1.2838679659998888,
            "model": "job-output.txt",
            "scores": [
                [20, 0.0],
                [21, 0.4999999999999999],
                [22, 0.0],
                [100, 0.0],
                [101, 0.8],
            ],
            "lines": [
                "Job: sf-ci-functional-minimal",
                "Pipeline: check",
                "Executor: ze04.softwarefactory-project.io",
                "install-server | TASK [Exec resources apply]",
                "install-server | fatal: [managesf.sfdomain.com]: FAILED!",
            ],
            "mean_distance": 0.25,
        },
        "logs/managesf.sfdomain.com/var/log/messages": {
            "file_url": "logs/managesf.sfdomain.com/var/log/messages",
            "test_time": 2.270206835000863,
            "model": "log/messages",
            "scores": [
                [2811, 0.0],
                [2812, 0.0],
                [2813, 0.09075879068336479],
                [2814, 0.5596144939494555],
                [2815, 0.0]
            ],
            "lines": [
                "Aug  6 07:02:30 managesf python: ansible-command Invoked",
                "Aug  6 07:02:30 managesf python: ansible-file Invoked",
                "Aug  6 07:02:30 managesf python: ansible-command Invoked",
                "Aug  6 07:02:30 managesf python: ansible-command Error",
                "Aug  6 07:02:34 managesf python2: ansible-command Invoked",
            ],
            "mean_distance": 0.13007465692656406
        },
    },
    "unknown_files": [
        [
            "logs/install-server/ansible/sfconfig.retry",
            "logs/install-server/ansible/sfconfig.retry"
        ]
    ],
    "models": {
        "job-output.txt": {
            "source_files": [
                "https://softwarefactory-project.io/logs/10/13310/1/gate/"
                "sf-ci-functional-minimal/9018205/job-output.txt.gz"
            ],
            "train_time": 3.051848716000677,
            "info": "8745 samples, 1048576 features",
            "uuid": "42"
        },
        "log/messages": {
            "source_files": [
                "https://softwarefactory-project.io/logs/10/13310/1/gate/"
                "sf-ci-functional-minimal/9018205/logs/managesf.sftests.com"
                "/var/log/messages"
            ],
            "train_time": 8.414538813000036,
            "info": "14448 samples, 1048576 features",
            "uuid": "23"
        },
    },
    "anomalies_count": 10,
    "baselines": [
        {
            "branch": "master",
            "project": "scl/zuul-distgit",
            "node_name": None,
            "start_time": "2018-08-06T04:21:42",
            "patchset": "1",
            "result": "SUCCESS",
            "voting": True,
            "change": 13310,
            "ref_url": "https://softwarefactory-project.io/r/13310",
            "newrev": None,
            "end_time": "2018-08-06T05:09:18",
            "ref": "refs/changes/10/13310/1",
            "duration": 2856.0,
            "pipeline": "gate",
            "job_name": "sf-ci-functional-minimal",
            "uuid": "901820598eec4502b3a9354caee43016",
            "log_url": ("https://softwarefactory-project.io/logs/10/13310/1/"
                        "gate/sf-ci-functional-minimal/9018205/"),
            "local_path": "/tmp/lr/_baselines/sf-ci-functional-minimal/9018205"
        }
    ],
    "targets": [
        {
            "branch": "master",
            "project": "software-factory/sf-config",
            "node_name": None,
            "start_time": "2018-08-06T06:49:10",
            "patchset": "1",
            "result": "FAILURE",
            "voting": True,
            "change": 13312,
            "ref_url": "https://softwarefactory-project.io/r/13312",
            "newrev": None,
            "end_time": "2018-08-06T07:05:10",
            "ref": "refs/changes/12/13312/1",
            "duration": 960.0,
            "pipeline": "check",
            "job_name": "sf-ci-functional-minimal",
            "uuid": "b252ec45f0524b49ab91e3fe60781091",
            "log_url": ("https://softwarefactory-project.io/logs/12/13312/1/"
                        "check/sf-ci-functional-minimal/b252ec4/"),
            "local_path": "/tmp/lr/_targets/sf-ci-functional-minimal/b252ec4"
        }
    ],
    "training_lines_count": 90664,
    "testing_lines_count": 8839,
    "outlier_lines_count": 55,
    "reduction": 99.37775766489422,
    "total_time": 4.587375126000552,
    "train_command": "logreduce job-train --job sf-ci-functional model.clf",
    "test_command": "logreduce job-run model.clf https://logs.sf/..."
}
