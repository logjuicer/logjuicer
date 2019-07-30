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
import re
import requests

import paho.mqtt.client as mqtt
import logreduce.server.utils as utils


class MQTTWorker:
    log = logging.getLogger("logreduce.MQTTWorker")

    def __init__(self, config):
        self.config = config.get("mqtt", {})
        self.jobs = list(map(
            re.compile, self.config.get("jobs", [r".*"])))
        self.tenants = list(map(
            re.compile, self.config.get("tenants", [r".*"])))
        self.client = None

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

    def on_connect(self, client, userdata, flags, rc):
        self.log.info("Connected with result code " + str(rc))
        client.subscribe(self.config.get("subscribe", "zuul/+/result/#"))

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

        self.processFailure(build)

    def processFailure(self, build):
        self.log.info("Processing build %s for job %s",
                      build["uuid"], build["job_name"])


def main():
    config = utils.usage("mqtt")
    utils.run([MQTTWorker(config)])


if __name__ == "__main__":
    main()
