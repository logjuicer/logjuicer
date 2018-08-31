# Copyright 2018 Red Hat, Inc.
#
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

import argparse
import json
import pprint
import urllib.request


def prepare_report(result, name="test-anomaly", reporter="anon"):
    """Convert logreduce report for server submission"""
    if len(result['targets']) != 1:
        raise RuntimeError("Can only single target report")
    report = {
        'name': name,
        'target': result['targets'][0],
        'baselines': result['baselines'],
        'train_command': result.get('train_command'),
        'test_command': result.get('test_command'),
        'reporter': reporter,
        'models': [],
        'logs': [],
    }
    models_used = {}
    for name, data in result["files"].items():
        if data['scores']:
            # Remove duplicate scores for splitted ansible lines
            last_pos = None
            scores = []
            for score in data['scores']:
                if last_pos != score[0]:
                    scores.append(score)
                last_pos = score[0]
            report["logs"].append({
                'path': name,
                'model': data['model'],
                'scores': scores,
                'test_time': data['test_time'],
            })
            models_used[data['model']] = result['models'][data['model']]
    for name, data in models_used.items():
        model = data
        model['name'] = name
        report['models'].append(model)

    return report


def main():
    parser = argparse.ArgumentParser(
        description="Push result to a logreduce-server")
    parser.add_argument("--server", required=True, metavar="URL",
                        help="The server api url.")
    parser.add_argument("--reporter", default="anon",
                        help="The reporter name.")
    parser.add_argument("--name", default="A new anomaly",
                        help="The anomaly name.")
    parser.add_argument("report_file",
                        help="The logreduce json report file to submit.")
    args = parser.parse_args()
    report = prepare_report(
        result=json.load(open(args.report_file)),
        name=args.name,
        reporter=args.reporter)
    req = urllib.request.Request(
        args.server,
        data=json.dumps(report).encode('utf8'),
        method='PUT',
        headers={'Content-Type': 'application/json'})
    response = urllib.request.urlopen(req)
    data = json.loads(response.read().decode('utf-8'))
    pprint.pprint(data)


if __name__ == "__main__":
    main()
