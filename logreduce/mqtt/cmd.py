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

import json
import logging
import os
import re
import requests
import subprocess
import threading
import urllib.parse

from queue import Queue, Empty
from logreduce.html_output import render_html
import paho.mqtt.client as mqtt
import logreduce.server.utils as utils
import logreduce.utils
import logreduce.worker


class MQTTWorker:
    log = logging.getLogger("logreduce.MQTTWorker")

    # Managements interface
    def __init__(self, config):
        self.config = config
        self.jobs = list(map(
            re.compile, self.config.get("mqtt", {}).get("jobs", [r".*"])))
        self.tenants = list(map(
            re.compile, self.config.get("mqtt", {}).get("tenants", [r".*"])))
        self.client = None
        self.worker = None
        self.alive = None
        self.queue = Queue(maxsize=10)
        self.model_dest = self.config["mqtt"].get("model_dest", "").rstrip('/')
        self.log_dest = self.config["mqtt"].get("log_dest", "").rstrip('/')
        self.do_rsync = self.config["mqtt"].get("rsync", False)
        self.only_model = self.config["mqtt"].get("only_model", False)
        self.filters = self.config.get("filters", {})

    def start(self):
        self.log.info("Starting MQTTWorker")
        self.client = mqtt.Client()
        self.client.connect(
            self.config["mqtt"]["url"], self.config["mqtt"].get("port", 1883))
        self.client.on_connect = self.on_connect
        self.client.on_message = self.on_message
        self.client.loop_start()

    def stop(self):
        self.log.info("Stopping MQTTWorker")
        if self.client:
            self.client.loop_stop()
            self.client = None
        if self.alive:
            self.alive = False
        if self.worker:
            self.worker.join()
            self.worker = None

    # MQTT interface
    def on_connect(self, client, userdata, flags, rc):
        self.log.info("Connected with result code " + str(rc))
        client.subscribe(self.config.get("subscribe", "zuul/+/result/#"))
        if not self.worker:
            self.alive = True
            self.worker = threading.Thread(target=self.processBuilds)
            self.worker.start()

    def on_message(self, client, userdata, msg):
        try:
            topic, payload = msg.topic, json.loads(msg.payload.decode('utf-8'))
        except Exception:
            self.log.exception("Couldn't decode message %s from %s",
                               msg.payload, msg.topic)
            return
        if not any(map(lambda x: x.match(payload["tenant"]), self.tenants)):
            return
        self.log.debug("Received %s %s", topic, payload)
        for build in payload.get("buildset", {}).get("builds", []):
            if build.get("result", "Null") == "FAILURE" and any(map(
                    lambda x: x.match(build.get("job_name")), self.jobs)):
                self.checkFailure(build)

    def checkFailure(self, build):
        if not build.get("log_url", "").endswith("/"):
            self.log.debug(
                "%s: Log url doesn't end with a /", build.get("log_url"))
            return
        if any(map(lambda x: requests.head(build.get("log_url") + x).ok,
                   ("log-classify.html", "report.html"))):
            self.log.debug("%s: report already built", build["log_url"])
            return

        try:
            self.queue.put(build)
        except Exception:
            self.log.exception("Failed to queue processing of %s", build)

    # Logreduce process interface
    def processBuilds(self):
        while self.alive:
            try:
                build = self.queue.get(block=True, timeout=1)
            except Empty:
                continue
            if build is None:
                continue
            try:
                self.processFailure(build)
            except Exception:
                self.log.exception("Failed to process %s", build)
            self.queue.task_done()

    def processFailure(self, build):
        self.log.info("Processing build %s for job %s",
                      build["uuid"], build["job_name"])
        try:
            phase = 'lookup'
            process = logreduce.worker.Process(self.config, {
                "uuid": build["uuid"],
                "path": "logs/",
                "name": build["uuid"],
                "reporter": "mqtt",
                "per-project": False,
                "exclude-files": self.filters.get("exclude_files"),
                "exclude-paths": self.filters.get("exclude_paths"),
                "exclude-lines": self.filters.get("exclude_lines"),
            })
            if self.only_model and process.loadModel():
                self.log.info("%s: model already built %s", build["uuid"], process.mf)
                return
            phase = 'train'
            process.train()
            phase = 'test'
            process.test()
        except Exception:
            self.log.exception("%s: failed at phase %s", build["uuid"], phase)
            return

        buildReportHtml = os.path.join(
            self.config.get("server", {}).get("logserver_folder", "/tmp"),
            build["uuid"] + ".html")
        buildReportJson = os.path.join(
            self.config.get("server", {}).get("logserver_folder", "/tmp"),
            build["uuid"] + ".json")
        with open(buildReportHtml, "w") as htmlFile:
            htmlFile.write(render_html(process.report))
        with open(buildReportJson, "w") as jsonFile:
            jsonFile.write(logreduce.utils.json_dumps(process.report))

        files = []
        if self.model_dest:
            files.append((
                os.path.dirname(process.mf) + "/",
                os.path.join(self.model_dest, build["job_name"]) + "/"
            ))
        if self.log_dest:
            files.append((
                buildReportHtml,
                os.path.join(
                    self.log_dest,
                    urllib.parse.urlsplit(build["log_url"]).path[1:],
                    "report.html")
            ))

        for src, dst in files:
            cmd = ["rsync", "-av", "--chmod=D755,F644", src, dst]
            try:
                self.log.info("Running %s" % " ".join(cmd))
                if self.do_rsync and subprocess.Popen(cmd).wait():
                    self.log.error("Command failed")
            except Exception:
                self.log.exception("Failed to run command")


def main():
    config = utils.usage("mqtt")
    utils.run([MQTTWorker(config)])


if __name__ == "__main__":
    main()
