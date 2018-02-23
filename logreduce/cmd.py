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
import time

import logreduce.utils

from logreduce.process import OutliersDetector
from logreduce.html_output import render_html
from logreduce.models import models


def usage():
    p = argparse.ArgumentParser()
    p.add_argument("--debug", action="store_true", help="Print debug")
    sub = p.add_subparsers()

    model_check = sub.add_parser("model-check", help="Check if model is valid")
    model_check.add_argument("--max_age", type=int, default=7,
                             help="Maximum age of a model")
    model_check.add_argument("file")
    model_check.set_defaults(func=model_check_action)

    model_build = sub.add_parser("model-build", help="Build a model")
    model_build.add_argument("--job")
    model_build.add_argument("--branch")
    model_build.add_argument("--project")
    model_build.add_argument("--pipeline")
    model_build.add_argument("--count")
    model_build.add_argument("--zuul-web")
    model_build.set_defaults(func=model_build_action)

    logs_get = sub.add_parser("log-get", help="Download logs")
    logs_get.add_argument("url")
    logs_get.set_defaults(func=logs_get_action)

    r = sub.add_parser("run", help="Detect anomalies")
    r.set_defaults(func=run_action)

    r.add_argument("--ignore-file", nargs='+',
                   help="Filename (basename) regexp")

    r.add_argument("--model-type", default="hashing_nn",
                   choices=list(models.keys()))

    r.add_argument("--html", metavar="FILE", help="Render html result")
    r.add_argument("--json", metavar="FILE", help="Render json result")

    r.add_argument("--model", metavar="FILE", help="Load a previous model")

    r.add_argument("--threshold", default=0.2, type=float,
                   help="Outlier distance threshold, set to 0.0 to display "
                        "all log, 1.0 to only display clear anomalies")

    r.add_argument("--merge-distance", default=5, type=int,
                   help="Distance between chunks to merge in a continuous one")
    r.add_argument("--before-context", default=3, type=int,
                   help="Amount of lines to include before the anomaly")
    r.add_argument("--after-context", default=1, type=int,
                   help="Amount of lines to include after the anomaly")
    r.add_argument("--context-length", type=int,
                   help="Set both before and after context size")

    r.add_argument("dir", nargs='+',
                   help="[baseline] target")
    args = p.parse_args()
    logreduce.utils.setup_logging(args.debug)
    return args


def model_check_action(args):
    ...


def model_build_action(args):
    ...


def logs_get_action(args):
    ...


def run_action(args):
    log = logging.getLogger("logreduce")
    start_time = time.monotonic()
    if len(args.dir) == 1 and not args.model:
        print("baseline or model needs to be used")
        exit(1)
    if args.ignore_file:
        logreduce.utils.IGNORE_FILES.extend(args.ignore_file)
    if args.context_length is not None:
        args.before_context = args.context_length
        args.after_context = args.context_length

    if args.html or args.json:
        args.console_output = False
    else:
        args.console_output = True
    # test html_output, only re-rerender
#    output = json.loads(open(args.json, "r").read())
#    open(args.html, "w").write(render_html(output))
#    exit(0)

    if args.model:
        clf = OutliersDetector.load(args.model)
        args.baseline = [args.model]
    else:
        clf = OutliersDetector(args.model_type)
        clf.train(args.dir[:-1])

    output = clf.process(args.dir[-1],
                         float(args.threshold),
                         args.merge_distance,
                         args.before_context,
                         args.after_context,
                         args.console_output)

    output["total_time"] = time.monotonic() - start_time
    if args.json:
        open(args.json, "w").write(json.dumps(output))
    if args.html:
        open(args.html, "w").write(render_html(output))
    if args.console_output:
        log.info("%02.2f%% reduction (from %d lines to %d)" % (
            output["reduction"],
            output["testing_lines_count"],
            output["outlier_lines_count"]
        ))


def main():
    args = usage()
    args.func(args)


if __name__ == "__main__":
    main()
