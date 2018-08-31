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

import logging
import urllib.parse
import os

import logreduce.download
import logreduce.server.client
import logreduce.server.rpc as rpc
import logreduce.server.utils as utils
from logreduce.process import Classifier


class Process:
    """Process a UserReport"""
    log = logging.getLogger("logreduce.worker.Process")

    def __init__(self, kwargs, request):
        self.kwargs = kwargs
        self.cache_only = kwargs["server"].get("cache_only", False)
        self.logserver_folder = kwargs["server"].get(
            "logserver_folder", "~/logs")
        self.request = request
        zuul_url = self.kwargs["server"].get("zuul_url")
        if request.get("url"):
            zuul_url = request["url"]
        if not zuul_url:
            raise RuntimeError("No Zuul API URL")

        #################################
        # Step 1, get build information #
        #################################
        self.zuul_url = zuul_url.rstrip('/')
        builds = logreduce.download.ZuulBuilds(self.zuul_url).get(
            uuid=request["uuid"])
        if len(builds) == 0:
            raise RuntimeError("Unknown build %s/builds?uuid=%s" % (
                self.zuul_url, request["uuid"]))
        if len(builds) != 1:
            raise RuntimeError("Couldn't find build (%s)" % builds)
        # extract build informations
        self.build = builds[0]
        self.job = self.build["job_name"]
        self.project = self.build["project"]
        self.branch = self.build["branch"]
        self.prj = self.project.replace('/', '_')
        self.brh = self.branch.replace('/', '_')
        self.lib_path = os.path.join(
            self.kwargs["server"]["models_folder"],
            urllib.parse.urlsplit(self.build["log_url"]).netloc)

    def train(self):
        #################################
        # Step 2, check for local model #
        #################################
        def load_model(model_file):
            try:
                if os.path.isfile(model_file):
                    clf = Classifier.load(model_file)
                    self.log.info("Re-using %s", model_file)
                    return clf, model_file
            except Exception as e:
                self.log.warning("Can't re-use %s (%s)", model_file, e)
            return None, None
        # check for per project/branch model built
        self.clf, self.mf = load_model(os.path.join(
            self.lib_path, self.job, "%s-%s.clf" % (self.prj, self.brh)))
        if self.clf:
            return
        # check for per project model built
        self.clf, self.mf = load_model(os.path.join(
            self.lib_path, self.job, "%s.clf" % (self.prj)))
        if self.clf:
            return
        # check for per job/branch model built
        self.clf, self.mf = load_model(os.path.join(
            self.lib_path, self.job, "%s.clf" % self.brh))
        if self.clf:
            return
        # check for per job model built
        self.clf, self.mf = load_model(
            os.path.join(self.lib_path, "%s.clf" % self.job))
        if self.clf:
            return

        # No model found, let's try to build a new one

        ###################################
        # Step 3, look for model baseline #
        ###################################
        train_args = [
            "--job", self.job, "--zuul-web", self.zuul_url, "--count", "2"]

        def get_baselines(**kwargs):
            results = logreduce.download.ZuulBuilds(self.zuul_url).get(
                job=self.job,
                result='SUCCESS',
                count=2,
                **kwargs)
            if results:
                for k, v in kwargs.items():
                    train_args.extend(["--%s" % k, v])
            return results
        # check for project and pipeline
        for pipeline in ["periodic", "gate", self.build["pipeline"]]:
            baselines = get_baselines(
                pipeline=pipeline, project=self.project, branch=self.branch)
            if baselines:
                self.model_file = os.path.join(
                    self.lib_path, self.job,
                    "%s-%s.clf" % (self.prj, self.brh))
        # todo: look for project without branch filter
        if not baselines:
            # check for pipeline
            for pipeline in ["periodic", "gate", self.build["pipeline"]]:
                baselines = get_baselines(
                    pipeline=pipeline, branch=self.branch)
                if baselines:
                    self.model_file = os.path.join(
                        self.lib_path, self.job, "%s.clf" % self.brh)
            if not baselines:
                # check for just job
                baselines = get_baselines()
                if baselines:
                    self.model_file = os.path.join(
                        self.lib_path, "%s.clf" % self.job)

        if not baselines:
            raise RuntimeError(
                "Couldn't find baselines for build (%s)" % self.build)

        ###################################
        # Step 4, Download baselines logs #
        ###################################
        if self.request.get('path'):
            train_args.extend(["--include-path", self.request['path']])
        for baseline in baselines:
            if baseline['log_url'][-1] != "/":
                baseline['log_url'] += "/"
            urlsplit = urllib.parse.urlsplit(baseline['log_url'])
            logspath = os.path.join(
                self.logserver_folder, urlsplit.netloc, urlsplit.path[1:])

            os.makedirs(logspath, mode=0o755, exist_ok=True)
            sub_paths = ["job-output.txt.gz", "zuul-info/inventory.yaml"]
            if self.request.get('path'):
                sub_paths.append(self.request['path'].lstrip('/'))
            for sub_path in sub_paths:
                url = os.path.join(baseline['log_url'], sub_path)
                if self.cache_only:
                    continue
                logreduce.download.RecursiveDownload(
                    url,
                    logspath,
                    trim=baseline['log_url'],
                    exclude_files=logreduce.utils.DEFAULT_IGNORE_FILES,
                    exclude_paths=logreduce.utils.DEFAULT_IGNORE_PATHS,
                    exclude_extensions=logreduce.utils.BLACKLIST_EXTENSIONS
                ).wait()
            baseline['local_path'] = logspath

        ################################
        # Step 5, Train and save model #
        ################################
        train_args.append('/'.join(self.model_file.split('/')[-2:]))
        self.mf = self.model_file
        self.clf = Classifier("hashing_nn")
        self.log.debug("Starting training of %s for %s", baselines, train_args)
        self.clf.train(
            baselines, command=["logreduce", "job-train"] + train_args)
        self.log.debug("Saving")
        self.clf.save(self.model_file)

    def test(self):
        test_args = []
        ################################
        # Step 6, Download target logs #
        ################################
        urlsplit = urllib.parse.urlsplit(self.build['log_url'])
        logspath = os.path.join(
            self.logserver_folder, urlsplit.netloc, urlsplit.path[1:])

        os.makedirs(logspath, mode=0o755, exist_ok=True)
        sub_paths = ["job-output.txt.gz", "zuul-info/inventory.yaml"]
        if self.request.get('path'):
            test_args.extend(["--include-path", self.request["path"]])
            sub_paths.append(self.request['path'].lstrip('/'))
        for sub_path in sub_paths:
            url = os.path.join(self.build['log_url'], sub_path)
            if self.cache_only:
                continue
            logreduce.download.RecursiveDownload(
                url,
                logspath,
                trim=self.build['log_url'],
                exclude_files=logreduce.utils.DEFAULT_IGNORE_FILES,
                exclude_paths=logreduce.utils.DEFAULT_IGNORE_PATHS,
                exclude_extensions=logreduce.utils.BLACKLIST_EXTENSIONS
            ).wait()
        self.build['local_path'] = logspath
        test_args.extend([
            '--zuul-web', self.zuul_url,
            '/'.join(self.mf.split('/')[-2:]),
            self.build['log_url']
        ])

        ###############################
        # Step 7, Run target analysis #
        ###############################
        report = self.clf.process(
            self.build,
            threshold=float(0.2),
            merge_distance=5,
            before_context=3,
            after_context=1,
            console_output=False,
            command=["logreduce", "job-run"] + test_args)

        #################################################
        # Step 8, Convert report for database injestion #
        #################################################
        return logreduce.server.client.prepare_report(
            report,
            name=self.request['name'],
            reporter=self.request['reporter'])


class Worker(rpc.Listener):
    log = logging.getLogger("logreduce.Worker")
    name = 'worker'

    def handle_process(self, request):
        """Handle process job submitted by the Api server"""
        self.log.info("Processing [%s]" % request)
        try:
            phase = 'lookup'
            process = Process(self.kwargs, request)
            phase = 'train'
            process.train()
            phase = 'test'
            result = {"report": process.test()}
        except Exception as e:
            error = "%s failed (%s)" % (phase, e)
            self.log.exception("%s: %s", request["uuid"], error)
            result = {"error": error}
        return result


def main():
    config = utils.usage("worker")
    services = []
    if config.get('gearman', {}).get('start'):
        services.append(rpc.Server(**config["gearman"]))

    utils.run([Worker(server=config["server"], **config["gearman"])])
