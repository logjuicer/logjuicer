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

import collections
import logging
import time

import cherrypy

import logreduce.server.model as model
import logreduce.server.rpc as rpc
import logreduce.server.utils as utils


class Api(object):
    log = logging.getLogger("logreduce.Api")
    """The Api object implements REST method."""

    def __init__(self, **kwargs):
        self.db = model.Db(kwargs.get("dburi", "sqlite:///logreduce.sqlite"))
        self.rpc = ServerWorker(self, **kwargs["gearman"])

    @cherrypy.expose
    @cherrypy.config(**{'tools.cors.on': True})
    @cherrypy.tools.json_out(content_type='application/json; charset=utf-8')
    def status(self):
        """Return german status, running jobs and jobs history"""
        return {
            'functions': self.rpc.getFunctions(),
            'jobs': self.rpc.jobs,
            'date': time.time(),
            'history': list(self.rpc.history),
        }


class ServerWorker(rpc.Listener):
    """Handle gearman job from api"""
    log = logging.getLogger("logreduce.ServerWorker")
    name = 'Server'
    # Also start a client to submit job
    client = True

    def __init__(self, api, **kwargs):
        super().__init__(**kwargs)
        self.api = api
        self.jobs = {}
        self.history = collections.deque(maxlen=17)

    def getFunctions(self):
        """Return the gearman status"""
        functions = {}
        for connection in self.worker.active_connections:
            try:
                req = self.StatusAdminRequest()
                connection.sendAdminRequest(req, timeout=300)
            except Exception:
                self.log.exception("Exception while listing functions")
                self.worker._lostConnection(connection)
                continue
            for line in req.response.decode('utf8').split('\n'):
                parts = [x.strip() for x in line.split('\t')]
                if len(parts) < 4:
                    continue
                # parts[0] - function name
                # parts[1] - total jobs queued (including building)
                # parts[2] - jobs building
                # parts[3] - workers registered
                data = functions.setdefault(parts[0], [0, 0, 0])
                for i in range(3):
                    data[i] += int(parts[i + 1])
        return functions


class Server(object):
    log = logging.getLogger("logreduce.Server")

    def __init__(self, addr="0.0.0.0", port=20004, tests=False, **kwargs):
        self.api = Api(**kwargs)
        route_map = cherrypy.dispatch.RoutesDispatcher()
        # graphql = logreduce.server.graphql.GraphQL(api.db)
        # route_map.connect('api', '/graphql',
        #                   controller=graphql, action='execute')

        route_map.connect('api', '/api/status',
                          controller=self.api, action='status',
                          conditions=dict(method=["GET"]))

        def CORS():
            if cherrypy.request.method == 'OPTIONS':
                cherrypy.response.headers[
                    'Access-Control-Allow-Methods'] = 'GET,POST,PUT,DELETE'
                cherrypy.response.headers[
                    'Access-Control-Allow-Headers'] = 'content-type'
                cherrypy.response.headers['Access-Control-Allow-Origin'] = '*'
                return True
            else:
                cherrypy.response.headers['Access-Control-Allow-Origin'] = '*'
        cherrypy.tools.cors = cherrypy._cptools.HandlerTool(CORS)

        conf = {
            '/': {
                'request.dispatch': route_map,
                'tools.cors.on': True,
                'tools.response_headers.headers': [
                    ('Access-Control-Allow-Origin', '*')],
            }
        }
        if not tests:
            cherrypy.config.update({
                'global': {
                    'environment': 'production',
                    'server.socket_host': addr,
                    'server.socket_port': int(port),
                },
            })
        cherrypy.tree.mount(self.api, '/', config=conf)

    def start(self, loop=False):
        self.log.debug("starting")
        self.api.rpc.start()
        cherrypy.engine.start()
        if loop:
            cherrypy.engine.block()

    def stop(self):
        self.log.debug("stopping")
        cherrypy.engine.exit()
        self.api.rpc.stop()


def main():
    config = utils.usage("server")
    services = []
    if config.get('gearman', {}).get('start'):
        services.append(rpc.Server(**config["gearman"]))
    config["server"]["gearman"] = config["gearman"]

    services.append(Server(**config["server"]))
    utils.run(services)
