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

# Code adapted from zuul-ci.org to abstract gearman

import os
import signal
import threading
import json
import time
import traceback

import gear


class Server:
    def __init__(self, **kwargs):
        self.kwargs = kwargs

    def start(self):
        pipe_read, pipe_write = os.pipe()
        child_pid = os.fork()
        if child_pid == 0:
            os.close(pipe_write)
            host = self.kwargs.get('addr')
            port = int(self.kwargs.get('port', 4730))
            gear.Server(
                port,
                host=host,
                keepalive=True,
                tcp_keepidle=300,
                tcp_keepintvl=60,
                tcp_keepcnt=5)
            # Keep running until the parent dies:
            pipe_read = os.fdopen(pipe_read)
            try:
                pipe_read.read()
            except KeyboardInterrupt:
                pass
            os._exit(0)
        else:
            os.close(pipe_read)
            self.gear_server_pid = child_pid
            self.gear_pipe_write = pipe_write

    def stop(self):
        if self.gear_server_pid:
            os.kill(self.gear_server_pid, signal.SIGKILL)


class Client(object):
    def __init__(self, addr, port, **kwargs):
        if addr == '0.0.0.0':
            addr = '127.0.0.1'
        self.addr = addr
        self.port = port
        self.kwargs = kwargs

    def start(self):
        self.gearman = gear.Client()
        self.gearman.addServer(self.addr, self.port)
        self.log.debug("Waiting for server")
        self.gearman.waitForServer()
        self.log.info("Connected")

    def stop(self):
        self.gearman.shutdown()

    def submitJob(self, name, data, wait=False):
        self.log.debug("Submitting job %s with data %s" % (name, data))
        job = gear.TextJob('logreduce:' + name,
                           json.dumps(data),
                           unique=str(time.time()))
        self.gearman.submitJob(job, timeout=300)

        if wait:
            self.log.debug("Waiting for job completion")
            while not job.complete:
                time.sleep(0.1)
            if job.exception:
                raise RuntimeError(job.exception)
            self.log.debug("Job complete, success: %s" % (not job.failure))
            if job.failure:
                return None
            else:
                return json.loads(job.data[0])
        return job


class Listener(Client):
    StatusAdminRequest = gear.StatusAdminRequest
    client = False

    def start(self):
        if self.client:
            super().start()
        self._running = True
        self.worker = gear.TextWorker('Logreduce ' + self.name + ' Listener')
        self.worker.addServer(self.addr, self.port)
        self.log.debug("Waiting for server")
        self.worker.waitForServer()
        self.log.debug("Registering")
        self.register()
        self.thread = threading.Thread(target=self.run)
        self.thread.daemon = True
        self.thread.start()

    def stop(self):
        self.log.debug("Stopping")
        self._running = False
        self.worker.shutdown()
        self.log.debug("Stopped")
        if self.client:
            super().stop()

    def join(self):
        self.thread.join()

    def run(self):
        self.log.debug("Starting RPC listener")
        while self._running:
            try:
                job = self.worker.getJob()
                self.log.debug("Received job %s" % job.name)
                z, jobname = job.name.split(':')
                attrname = 'handle_' + jobname
                if hasattr(self, attrname):
                    f = getattr(self, attrname)
                    if callable(f):
                        try:
                            res = f(**json.loads(job.arguments))
                            job.sendWorkComplete(json.dumps(res))
                        except Exception:
                            self.log.exception("Exception while running job")
                            job.sendWorkException(traceback.format_exc())
                    else:
                        job.sendWorkFail()
                else:
                    job.sendWorkFail()
            except gear.InterruptedError:
                return
            except Exception:
                self.log.exception("Exception while getting job")

    def register(self):
        for fn in dir(self):
            if fn.startswith("handle_"):
                self.worker.registerFunction(
                    "logreduce:%s" % fn.replace("handle_", ""))
