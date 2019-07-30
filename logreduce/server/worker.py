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

import logreduce.worker
import logreduce.server.client
import logreduce.server.rpc as rpc
import logreduce.server.utils as utils


class Worker(rpc.Listener):
    log = logging.getLogger("logreduce.Worker")
    name = 'worker'

    def handle_process(self, request):
        """Handle process job submitted by the Api server"""
        self.log.info("Processing [%s]" % request)
        try:
            phase = 'lookup'
            process = logreduce.worker.Process(self.kwargs, request)
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
