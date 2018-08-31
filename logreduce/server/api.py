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
import os.path
import time
import urllib.parse

import sqlalchemy.orm.exc
import cherrypy
import requests

from logreduce.utils import open_file
import logreduce.server.model as model
import logreduce.server.rpc as rpc
import logreduce.server.utils as utils


class Api(object):
    log = logging.getLogger("logreduce.Api")
    """The Api object implements REST method."""

    def __init__(self, **kwargs):
        self.db = model.Db(kwargs.get("dburi", "sqlite:///logreduce.sqlite"))
        self.rpc = ServerWorker(self, **kwargs["gearman"])
        public_url = kwargs.get("public_url", "http://localhost:20004")
        if public_url[-1] == "/":
            public_url = public_url[:-1]
        self.public_url = public_url
        self.logserver_folder = kwargs.get("logserver_folder", "~/logs")

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

    @cherrypy.expose
    @cherrypy.config(**{'tools.cors.on': True})
    @cherrypy.tools.json_out(content_type='application/json; charset=utf-8')
    def list(self):
        """Return the anomalies list"""
        results = []
        with self.db.session() as session:
            for anomaly in session.query(model.Anomaly):
                results.append({
                    'uuid': anomaly.uuid,
                    'name': anomaly.name,
                    'status': anomaly.status,
                    'reporter': anomaly.reporter,
                    'report_date': anomaly.report_date.isoformat(),
                    'build': anomaly.build.toDict()
                })
        cherrypy.response.headers['Access-Control-Allow-Origin'] = '*'
        results.reverse()
        return results

    def _getAnomaly(self, session, anomaly_id):
        try:
            return session.query(
                model.Anomaly).filter_by(uuid=anomaly_id).one()
        except sqlalchemy.orm.exc.NoResultFound:
            raise cherrypy.HTTPError(404)

    @cherrypy.expose
    @cherrypy.config(**{'tools.cors.on': True})
    @cherrypy.tools.json_out(content_type='application/json; charset=utf-8')
    def get(self, anomaly_id):
        """Return a single anomaly"""
        with self.db.session() as session:
            anomaly = self._getAnomaly(session, anomaly_id)
            result = self.db.export(anomaly)
            urlsplit = urllib.parse.urlsplit(anomaly.build.log_url)
            logspath = os.path.join(
                self.logserver_folder, urlsplit.netloc, urlsplit.path[1:])
            for logfile in result.get('logfiles', []):
                # Get lines content
                logpath = os.path.join(logspath, logfile['path'])
                if os.path.exists(logpath):
                    loglines = []
                    try:
                        fobj = open_file(logpath)
                        while True:
                            line = fobj.readline()
                            if line == b'':
                                break
                            loglines.append(
                                line.decode('ascii', errors='ignore'))
                    except Exception:
                        self.log.exception("%s: couldn't read", logpath)
                    fobj.close()
                else:
                    self.log.warning("%s: doesn't exist", logpath)
                    self.rpc.download(
                        os.path.join(anomaly.build.log_url, logfile['path']),
                        logpath)
                    logfile['lines'].append("Log file download is in progress")
                    loglines = None
                for line in logfile['scores']:
                    if loglines:
                        try:
                            logfile['lines'].append(loglines[line[0]])
                        except IndexError:
                            logfile['lines'].append(
                                "file download is still in progress")

        # TODO: check if local cors is still needed
        resp = cherrypy.response
        resp.headers['Access-Control-Allow-Origin'] = '*'

        return result

    @cherrypy.expose
    @cherrypy.config(**{'tools.cors.on': True})
    @cherrypy.tools.json_in()
    @cherrypy.tools.json_out(content_type='application/json; charset=utf-8')
    def report(self):
        """Import a json file generated manually with the logreduce cli"""
        report = cherrypy.request.json
        self.log.info("Importing %s" % (report.get("name")))
        with self.db.session() as session:
            anomaly_uuid = self.db.import_report(session, report)
        cherrypy.response.headers['Access-Control-Allow-Origin'] = '*'
        return {
            'uuid': anomaly_uuid,
            'url': "%s/#/view/%s" % (self.public_url, anomaly_uuid)
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

    def download(self, url, dest):
        """When Api miss a file, this fire-and-forget a download job"""
        self.submitJob("download_log", {"url": url, "dest": dest})

    def handle_download_log(self, url, dest):
        """Handle download job"""
        self.log.info("Downloading %s to %s", url, dest)
        try:
            os.makedirs(os.path.dirname(dest), 0o755, exist_ok=True)
            r = requests.get(url)
            with open(dest, 'wb') as fd:
                for chunk in r.iter_content(chunk_size=4096):
                    fd.write(chunk)
        except Exception:
            self.log.exception("Download of %s failed", url)


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
        route_map.connect('api', '/api/anomaly',
                          controller=self.api, action='report',
                          conditions=dict(method=["PUT"]))
        route_map.connect('api', '/api/anomaly/{anomaly_id}',
                          controller=self.api, action='get',
                          conditions=dict(method=["GET"]))
        route_map.connect('api', '/api/anomalies',
                          controller=self.api, action='list',
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
