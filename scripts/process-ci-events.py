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
import subprocess
import sys
import os

import yaml

# Events:
test_event = {
    'id': '42',
    'result': 'FAILURE',
    'job': 'gate-tripleo-ci-centos-7-ovb-ha-oooq',
    'change': '42',
    'log_url': 'http://logs.openstack.org/69/501669/3/check-tripleo/'
               'gate-tripleo-ci-centos-7-ovb-ha-oooq/3c6c85f',
}


def execute(argv, stdout=None, stderr=None):
    print("Running: %s" % " ".join(argv))
    if subprocess.Popen(argv, stdout=stdout, stderr=stderr).wait():
        raise RuntimeError("%s: failed" % argv)


def train_model(job_name, model_name, last_periodic_jobs):
    # Fetch logs
    last_periodic_logs = []
    for job in last_periodic_jobs:
        execute(["./getthelogs", job])
        last_periodic_logs.extend(["--baseline", "/tmp/%s" % job[6:]])
    # Train the model
    execute(["logreduce", "--debug", "--save", model_name] +
            last_periodic_logs,
            stdout=open("%s.stdout" % model_name, "w"),
            stderr=open("%s.stderr" % model_name, "w"))


def process_event(event):
    if event['result'] != 'FAILURE':
        return
    # Check if the model exists
    model = "%s.clf" % event['job']
    if not os.path.exists("%s.clf" % event['job']):
        train_model(event['job'], model, event['last_periodic_jobs'])

    # Fetch failure logs
    execute(["./getthelogs", event['log_url']])

    # Run analysis
    execute(["logreduce", "--debug",
             "--json", "%s-%s.json" % (event['job'], event['id']),
             "--html", "%s-%s.html" % (event['job'], event['id']),
             "--load", model,
             "/tmp/%s" % event['log_url'][6:]],
            stdout=open("%s.stdout" % event['id'], "w"),
            stderr=open("%s.stderr" % event['id'], "w"))


def main():
    inf = yaml.safe_load(open(sys.argv[1]))
    event = {
        'id': os.path.basename(inf['last_failure'][0].rstrip('/')),
        'job': os.path.basename(sys.argv[1].replace('.yaml', '')),
        'change': 'XX',
        'log_url': inf['last_failure'][0],
        'last_periodic_jobs': inf['last_periodic_jobs'],
        'result': 'FAILURE',
    }
    process_event(event)


if __name__ == "__main__":
    main()
