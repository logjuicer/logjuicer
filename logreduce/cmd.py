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
import time

import logreduce.utils

from logreduce.process import OutliersDetector
from logreduce.html_output import render_html
from logreduce.models import models


def usage():
    p = argparse.ArgumentParser()
    p.add_argument("--debug", action="store_true", help="Print debug")
    p.add_argument("--ignore-file", nargs='+',
                   help="Filename (basename) regexp")

    p.add_argument("--model", default="bag-of-words_nn",
                   choices=list(models.keys()))

    p.add_argument("--html", metavar="FILE", help="Render html result")
    p.add_argument("--json", metavar="FILE", help="Render json result")

    p.add_argument("--save", metavar="FILE", help="Save the model")
    p.add_argument("--load", metavar="FILE", help="Load a previous model")

    p.add_argument("--threshold", default=0.2, type=float,
                   help="Outlier distance threshold, set to 0.0 to display "
                        "all log, 1.0 to only display clear anomalies")

    p.add_argument("--merge-distance", default=5, type=int,
                   help="Distance between chunks to merge in a continuous one")
    p.add_argument("--before-context", default=3, type=int,
                   help="Amount of lines to include before the anomaly")
    p.add_argument("--after-context", default=1, type=int,
                   help="Amount of lines to include after the anomaly")
    p.add_argument("--context-length", type=int,
                   help="Set both before and after context size")

    p.add_argument("--baseline", action='append', metavar="LOG",
                   help="Success logs path")
    p.add_argument("target", nargs='*', help="Failed logs path")
    args = p.parse_args()
    if not args.baseline and not args.load:
        print("baseline or load needs to be used")
        exit(1)
    if args.load and args.save:
        print("load and save can't be used together")
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

    return args


def main():
    start_time = time.monotonic()
    args = usage()
    log = logreduce.utils.setup_logging(args.debug)
    # test html_output, only re-rerender
#    output = json.loads(open(args.json, "r").read())
#    open(args.html, "w").write(render_html(output))
#    exit(0)

    if args.load:
        clf = OutliersDetector.load(args.load)
        args.baseline = [args.load]
    else:
        clf = OutliersDetector(args.model)
        clf.train(args.baseline)

    if args.save:
        clf.save(args.save)
        if not args.target:
            exit(0)

    if not args.target:
        log.error("No target found/specified")
        exit(1)

    output = clf.process(args.target, float(args.threshold),
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


if __name__ == "__main__":
    main()
