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
import os
import time

import logreduce.download
import logreduce.utils

from logreduce.process import Classifier
from logreduce.html_output import render_html
from logreduce.models import models


DEFAULT_ZUUL_WEB = os.environ.get(
    "ZUUL_WEB", "https://softwarefactory-project.io/zuul/local")


class Cli:
    def __init__(self):
        parser = self.usage()
        args = parser.parse_args()
        if not args.func:
            parser.print_help()
            exit(4)
        logreduce.utils.setup_logging(args.debug)
        kwargs = {}
        for k, v in args.__dict__.items():
            if k == "exclude_file":
                v.extend(logreduce.utils.DEFAULT_IGNORE_FILES)
            elif k == "exclude_path":
                v.extend(logreduce.utils.DEFAULT_IGNORE_PATHS)
            if k in ("logs_url", "model_file", "target_dir", "baselines_dir"):
                kwargs[k] = v
            else:
                self.__dict__[k] = v
        try:
            args.func(**kwargs)
        except RuntimeError:
            exit(4)

    def usage(self):
        parser = argparse.ArgumentParser()
        parser.set_defaults(func=None)
        parser.add_argument("--debug", action="store_true", help="Print debug")
        parser.add_argument("--tmp-dir", default=os.getcwd())

        # Common arguments
        def path_filters(s):
            s.add_argument("--include-path",
                           help="Logserver extra logs path")
            s.add_argument("--exclude-file", action='append', default=[],
                           help="Filename (basename) exclude regexp")
            s.add_argument("--exclude-path", action='append', default=[],
                           help="Path exclude regexp")

        def job_filters(s, job_required=True):
            s.add_argument("--job", required=job_required,
                           help="The job name")
            s.add_argument("--branch", default='master',
                           help="The branch name")
            s.add_argument("--pipeline", default="gate",
                           help="The pipeline to fetch baseline from, "
                           "e.g. gate or periodic")
            s.add_argument("--project",
                           help="Build a model per project")
            s.add_argument("--zuul-web", default=DEFAULT_ZUUL_WEB,
                           help="The zuul-web url (including the tenant name)")
            s.add_argument("--count", type=int, default=1,
                           help="The number of baseline to use")

        def report_filters(s):
            s.add_argument("--html", metavar="FILE", help="Render html result")
            s.add_argument("--threshold", default=0.2, type=float,
                           help="Anomalies distance threshold")
            s.add_argument(
                "--merge-distance", default=5, type=int,
                help="Distance between chunks to merge in a continuous one")
            s.add_argument(
                "--before-context", default=3, type=int,
                help="Amount of lines to include before the anomaly")
            s.add_argument(
                "--after-context", default=1, type=int,
                help="Amount of lines to include after the anomaly")
            s.add_argument(
                "--context-length", type=int,
                help="Set both before and after context size")

        def model_filters(s):
            s.add_argument("--max-age", type=int, default=7,
                           help="Maximum age of a model")
            s.add_argument("--model-type", default="hashing_nn",
                           choices=list(models.keys()),
                           help="The model type")

        # Sub command usages
        def model_check_usage(sub):
            s = sub.add_parser("model-check", help="Check if a model is valid")
            s.set_defaults(func=self.model_check)
            model_filters(s)
            s.add_argument("model_file")

        def model_build_usage(sub):
            s = sub.add_parser("model-build", help="Build a model")
            s.set_defaults(func=self.model_build)
            job_filters(s)
            path_filters(s)
            model_filters(s)
            s.add_argument("model_file")

        def model_run_usage(sub):
            s = sub.add_parser("model-run", help="Run a model")
            s.set_defaults(func=self.model_run)
            path_filters(s)
            report_filters(s)
            s.add_argument("model_file", metavar="FILE")
            s.add_argument("target_dir", metavar="DIR", nargs='+')

        def logs_get_usage(sub):
            s = sub.add_parser("logs-get", help="Get the logs")
            s.set_defaults(func=self.logs_get)
            path_filters(s)
            s.add_argument("logs_url", help="The logs file url")
            s.add_argument("target_dir", nargs='?')

        def logs_usage(sub):
            s = sub.add_parser("logs", help="Get the logs, build a model, "
                                            "report anomalies")
            s.set_defaults(func=self.logs_run)
            path_filters(s)
            job_filters(s, job_required=False)
            model_filters(s)
            report_filters(s)
            s.add_argument("logs_url")

        def diff_usage(sub):
            s = sub.add_parser("diff", help="Compare directories/files")
            report_filters(s)
            s.add_argument("--json", metavar="FILE",
                           help="Optional json output")
            s.add_argument("baselines_dir", nargs='+')
            s.add_argument("target_dir")
            s.set_defaults(func=self.diff_run)

        sub_parser = parser.add_subparsers()
        model_check_usage(sub_parser)
        model_build_usage(sub_parser)
        model_run_usage(sub_parser)
        logs_get_usage(sub_parser)
        logs_usage(sub_parser)
        diff_usage(sub_parser)
        return parser

    def model_check(self, model_file):
        max_age_sec = self.max_age * 24 * 3600
        if (not os.path.exists(model_file) or
                time.time() - os.stat(model_file).st_mtime > max_age_sec):
            raise RuntimeError("%s: does not exists or too old" % model_file)
        try:
            Classifier.check(open(model_file, 'rb'))
        except Exception as e:
            raise RuntimeError("%s: %s" % (model_file, e))

    def model_build(self, model_file):
        # Discover base-lines
        baselines = []
        for baseline in logreduce.download.ZuulBuilds(self.zuul_web).get(
                job=self.job,
                branch=self.branch,
                pipeline=self.pipeline,
                project=self.project,
                count=self.count):
            baselines.append(baseline)
        if not baselines:
            print("%s: couldn't find success in pipeline %s" % (
                self.job, self.pipeline))
            exit(4)
        baselines_paths = []
        url_prefixes = {}
        for baseline in baselines:
            if baseline[-1] != "/":
                baseline += "/"
            dest = os.path.join(
                self.tmp_dir, "_baselines", self.job, baseline.split('/')[-2])
            self.logs_get(baseline, dest)
            baselines_paths.append(dest)
            url_prefixes["%s/" % dest] = baseline

        # Train model
        clf = Classifier(
            self.model_type, self.exclude_path, self.exclude_file)
        clf.train(baselines_paths, url_prefixes)
        clf.save(model_file)
        print("%s: built with %s" % (model_file, " ".join(baselines)))

    def model_run(self, model_file, target_dir):
        clf = Classifier.load(model_file)
        clf.exclude_paths = self.exclude_path
        clf.exclude_files = self.exclude_file
        clf.test_prefix = self.include_path
        self._report(clf, target_dir)

    def _report(self, clf, target_dirs, target_source=None, json_file=None):
        if self.context_length is not None:
            self.before_context = self.context_length
            self.after_context = self.context_length

        console_output = True
        if json_file or self.html:
            console_output = False
        start_time = time.monotonic()
        output = clf.process(path=target_dirs,
                             path_source=target_source,
                             threshold=float(self.threshold),
                             merge_distance=self.merge_distance,
                             before_context=self.before_context,
                             after_context=self.after_context,
                             console_output=console_output)
        output["total_time"] = time.monotonic() - start_time
        if self.html:
            open(self.html, "w").write(render_html(output))
            open(self.html.replace(".html", ".json"), "w").write(
                json.dumps(output))
        if json_file is not None:
            open(json_file, "w").write(json.dumps(output))
        else:
            print("%02.2f%% reduction (from %d lines to %d)" % (
                output["reduction"],
                output["testing_lines_count"],
                output["outlier_lines_count"]
            ))

    def logs_get(self, logs_url, dest_dir=None):
        if logs_url[-1] != "/":
            logs_url += "/"
        if self.job is None:
            self.job = logs_url.split('/')[-3]
        if dest_dir is None:
            dest_dir = os.path.join(
                self.tmp_dir, "_targets", self.job, logs_url.split('/')[-2])
        os.makedirs(dest_dir, exist_ok=True)

        logs_path = ["job-output.txt.gz"]
        if self.include_path:
            logs_path.append(self.include_path)

        for sub_path in logs_path:
            url = os.path.join(logs_url, sub_path)
            logreduce.download.RecursiveDownload(
                url,
                dest_dir,
                trim=logs_url,
                exclude_files=self.exclude_file,
                exclude_paths=self.exclude_path,
                exclude_extensions=logreduce.utils.BLACKLIST_EXTENSIONS).wait()
        return dest_dir

    def logs_run(self, logs_url):
        if logs_url[-1] != "/":
            logs_url += "/"
        if self.job is None:
            self.job = logs_url.split('/')[-3]

        if "logs.openstack.org" in logs_url:
            self.zuul_web = "http://zuul.openstack.org"

        if self.project:
            model_file = os.path.join(
                "_baselines", self.project, "%s.clf" % self.job)
        else:
            model_file = os.path.join("_baselines", "%s.clf" % self.job)
        try:
            self.model_check(model_file)
        except RuntimeError:
            self.model_build(model_file)

        dest_dir = os.path.join(
            self.tmp_dir, "_targets", self.job, logs_url.split('/')[-2])
        self.logs_get(logs_url, dest_dir)

        clf = Classifier.load(model_file)
        clf.exclude_paths = self.exclude_path
        clf.exclude_files = self.exclude_file
        self._report(clf, dest_dir, logs_url)

    def diff_run(self, baselines_dir, target_dir):
        clf = Classifier()
        clf.train(baselines_dir)
        self._report(clf, target_dir, json_file=self.json)


def main():
    try:
        Cli()
    except RuntimeError as e:
        print(e)
        exit(4)


if __name__ == "__main__":
    main()
