#!/bin/env python3
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
import logging
import pprint
import os
import yaml

import numpy as np

from logreduce.bagofwords import BagOfWords
from logreduce.jenkins import Jenkins
from logreduce.utils import Tokenizer


def usage():
    p = argparse.ArgumentParser()
    p.add_argument("--debug", action="store_true", help="Print debug")
    p.add_argument("--debug-token", action="store_true",
                   help="Print tokenization process")

    p.add_argument("--output-format", default="text",
                   choices=["text", "json", "yaml", "pprint"])

    p.add_argument("--save", metavar="FILE", help="Save the model")
    p.add_argument("--load", metavar="FILE", help="Load a previous model")
    p.add_argument("--jenkins-url", help="Target a custom Jenkins service",
                   default="https://softwarefactory-project.io/jenkins")
    p.add_argument("--fetch-artifacts", action="store_true",
                   help="Fetch zuul-swift-upload artifacts (needs lftp)")

    p.add_argument("--max-distance", default=0.2, type=float,
                   help="Outlier distance threshold, set to 0.0 to display "
                        "all log, 1.0 to only display obvious anomalies")

    p.add_argument("--merge-distance", default=5, type=int,
                   help="Distance between chunks to merge in a continuous one")
    p.add_argument("--context-length", default=3, type=int,
                   help="Amount of lines to include before the anomaly")

    p.add_argument("--baseline", metavar="LOG", help="A success log")
    p.add_argument("target", nargs='*', help="The log to reduce")
    args = p.parse_args()
    if not args.baseline and not args.load:
        print("baseline or load needs to be used")
        exit(1)
    return args


def setup_logging(debug=False, debug_token=False):
    loglevel = logging.INFO
    if debug:
        loglevel = logging.DEBUG
    if debug_token:
        import logreduce.utils
        logreduce.utils.DEBUG_TOKEN = True
    logging.basicConfig(
        format='%(asctime)s %(levelname)-5.5s %(name)s - %(message)s',
        level=loglevel)
    return logging.getLogger("LogAnomaly")


def main():
    args = usage()
    log = setup_logging(args.debug, args.debug_token)
    jenkins = Jenkins(args.jenkins_url, args.fetch_artifacts)
    if args.load:
        clf = BagOfWords.load(args.load)
    else:
        clf = BagOfWords(args.max_distance, args.debug_token)

    if args.baseline:
        # Auto-target .fail file if baseline is a .good file
        if args.baseline.endswith(".good") and not args.target:
            fail_target = args.baseline.replace('.good', '.fail')
            if os.path.isfile(fail_target):
                log.info("Targetting %s" % fail_target)
                args.target = fail_target
        # Decode jenkins
        if args.baseline.startswith("jenkins:"):
            _, job_name = args.baseline.split(':', 1)
            if ":" in job_name:
                job_name, job_nr = job_name.split(':')
            else:
                job_nr = jenkins.get_last_success_nr(job_name)
            args.baseline = jenkins.get_logs(job_name, job_nr)
            if not args.target:
                log.info("Targetting last failed %s job" % job_name)
                args.target = ["jenkins:%s" % job_name]
        # Train the model
        clf.train(args.baseline)

    if args.save:
        clf.save(args.save)
        if not args.target:
            exit(0)

    if not args.target:
        log.error("No target found/specified")
        exit(1)

    for idx in range(len(args.target)):
        # Decode jenkins target
        if args.target[idx].startswith("jenkins:"):
            _, job_name = args.target[idx].split(':', 1)
            if ":" in job_name:
                job_name, job_nr = job_name.split(':')
            else:
                job_nr = jenkins.get_last_failed_nr(job_name)
            args.target[idx] = jenkins.get_logs(job_name, job_nr)

    output = {}
    for filename, outliers in clf.test(args.target, args.merge_distance,
                                       args.context_length):
        output[filename] = {
            'chunks': [],
            'scores': []
        }
        current_chunk = []
        current_score = []
        last_pos = None

        for pos, distance, outlier in outliers:
            if last_pos and pos - last_pos != 1:
                # New chunk
                output[filename]["chunks"].append("\n".join(current_chunk))
                output[filename]["scores"].append(float(np.mean(current_score)))
                current_chunk = []
                current_score = []
                if last_pos and args.output_format == "text":
                    print()

            # Clean ansible one-liner outputs
            for line in outlier[:-1].split(r'\n'):
                # Clear lines without content
                if len(Tokenizer.process(line)) < 5:
                    continue
                line = line.replace(r'\t', '\t')
                current_score.append(distance)
                current_chunk.append(line)
                if args.output_format == "text":
                    print("%1.3f | %s:%04d:\t%s" % (distance,
                                                    filename,
                                                    pos + 1,
                                                    line))

            last_pos = pos
        if current_chunk:
            output[filename]["chunks"].append("\n".join(current_chunk))
            output[filename]["scores"].append(float(np.mean(current_score)))

    if args.output_format == "pprint":
        pprint.pprint(output)
    elif args.output_format == "json":
        print(json.dumps(output))
    elif args.output_format == "yaml":
        print(yaml.safe_dump(output, default_flow_style=False))
    elif args.output_format != "text":
        raise RuntimeError("%s: unknown output-format" % args.output_format)


if __name__ == "__main__":
    main()
