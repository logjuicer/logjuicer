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

import numpy as np

import logreduce.utils

from logreduce.bagofwords import BagOfWords
from logreduce.html_output import render_html


def usage():
    p = argparse.ArgumentParser()
    p.add_argument("--debug", action="store_true", help="Print debug")
    p.add_argument("--debug-token", action="store_true",
                   help="Print tokenization process")
    p.add_argument("--update-cache", action="store_true",
                   help="Force re-download")
    p.add_argument("--ignore-file", nargs='+')

    p.add_argument("--model", default="simple",
                   choices=["simple", "lshf", "noop"])

    p.add_argument("--html", help="Render html result")
    p.add_argument("--json", help="Render json result")

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
                   help="Set both after and before size")

    p.add_argument("--baseline", action='append', metavar="LOG",
                   help="Success logs")
    p.add_argument("target", nargs='*', help="Failed logs")
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
        args.print_console = False
    else:
        args.print_console = True

    return args


def setup_logging(debug=False, debug_token=False):
    loglevel = logging.INFO
    if debug:
        loglevel = logging.DEBUG
    if debug_token:
        logreduce.utils.DEBUG_TOKEN = True
    logging.basicConfig(
        format='%(asctime)s %(levelname)-5.5s %(name)s - %(message)s',
        level=loglevel)
    return logging.getLogger("LogAnomaly")


def main():
    start_time = time.time()
    args = usage()
    log = setup_logging(args.debug, args.debug_token)

    if args.load:
        clf = BagOfWords.load(args.load)
        args.baseline = [args.load]
    else:
        clf = BagOfWords(args.model)
        clf.train(args.baseline)

    if args.save:
        clf.save(args.save)
        if not args.target:
            exit(0)

    if not args.target:
        log.error("No target found/specified")
        exit(1)

    output = {'files': {}}
    for filename, source_files, outliers in clf.test(args.target,
                                                     float(args.threshold),
                                                     args.merge_distance,
                                                     args.before_context,
                                                     args.after_context):
        file_info = output['files'].setdefault(filename, {
            'source_files': source_files,
            'chunks': [],
            'scores': [],
            'line_pos': [],
            'lines_count': 0,
        })
        current_chunk = []
        current_score = []
        current_pos = []
        last_pos = None
        log.debug("%s: compared with %s" % (filename, " ".join(source_files)))

        for pos, distance, outlier in outliers:
            distance = abs(float(distance))
            if last_pos and pos - last_pos != 1:
                # New chunk
                file_info["chunks"].append("\n".join(current_chunk))
                file_info["scores"].append(current_score)
                file_info["line_pos"].append(current_pos)
                file_info["lines_count"] += len(current_chunk)
                current_chunk = []
                current_score = []
                current_pos = []
                if last_pos and args.print_console:
                    print()

            # Clean ansible one-liner outputs
            for line in outlier[:-1].split(r'\n'):
                line = line.replace(r'\t', '\t')
                current_score.append(distance)
                current_chunk.append(line)
                current_pos.append(pos)
                if args.print_console:
                    print("%1.3f | %s:%04d:\t%s" % (distance,
                                                    filename,
                                                    pos + 1,
                                                    line))

            last_pos = pos
        if current_chunk:
            file_info["chunks"].append("\n".join(current_chunk))
            file_info["scores"].append(current_score)
            file_info["line_pos"].append(current_pos)
            file_info["lines_count"] += len(current_chunk)

        # Compute mean distances of outliers
        mean_distance = 0
        if file_info["scores"]:
            mean_distance = np.mean(np.hstack(file_info["scores"]))
        file_info["mean_distance"] = mean_distance

    output["files_sorted"] = sorted(output['files'].items(),
                                    key=lambda x: x[1]['mean_distance'],
                                    reverse=True)
    output["training_lines_count"] = clf.training_lines_count
    output["testing_lines_count"] = clf.testing_lines_count
    output["outlier_lines_count"] = clf.outlier_lines_count
    output["reduction"] = 100 - (output["outlier_lines_count"] /
                                 output["testing_lines_count"]) * 100
    output["total_time"] = time.time() - start_time
    output["baseline"] = args.baseline
    output["target"] = args.target

    if args.json:
        open(args.json, "w").write(json.dumps(output))
    if args.html:
        open(args.html, "w").write(render_html(output))
    if args.print_console:
        log.info("%02.2f%% reduction (from %d lines to %d)" % (
            output["reduction"],
            output["testing_lines_count"],
            output["outlier_lines_count"]
        ))


if __name__ == "__main__":
    main()
