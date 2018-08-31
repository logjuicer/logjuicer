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
import datetime
import json
import logging
import os.path
import time
import threading
import urllib.parse
import yaml

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
        self.dataset_folder = kwargs.get("dataset_folder", "~/anomalies")
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
    def delete(self, anomaly_id):
        """Delete an anomaly"""
        with self.db.session() as session:
            anomaly = self._getAnomaly(session, anomaly_id)
            session.delete(anomaly)
            session.commit()

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

    @cherrypy.expose
    @cherrypy.config(**{'tools.cors.on': True})
    @cherrypy.tools.json_in()
    @cherrypy.tools.json_out(content_type='application/json; charset=utf-8')
    def create(self):
        """Receive a user request to create a report"""
        request = cherrypy.request.json
        self.log.info("New request %s" % request)
        request = model.UserReport.schema(request)
        try:
            self.rpc.process(request)
            return {'message': 'Process scheduled'}
        except Exception:
            self.log.exception("oops")
            return {'message': 'Can not start process'}

    @cherrypy.expose
    @cherrypy.config(**{'tools.cors.on': True})
    @cherrypy.tools.json_in()
    @cherrypy.tools.json_out(content_type='application/json; charset=utf-8')
    def update(self, anomaly_id):
        """Update anomaly status"""
        self.log.info("Updating %s" % anomaly_id)
        request = cherrypy.request.json
        self.log.debug("Updating %s with %s" % (anomaly_id, request))
        if request.get("status"):
            if request["status"] not in ("reviewed", "archive"):
                return {'error': 'status can only be reviewed or archived'}
        elif request.get("copy"):
            if not request["copy"].get("name"):
                return {'error': 'a new name is required'}
        else:
            return {'error': 'invalid update request'}
        with self.db.session() as session:
            anomaly = self._getAnomaly(session, anomaly_id)
            if request.get("status"):
                if request["status"] == "reviewed":
                    anomaly.status = "reviewed"
                else:
                    # When status is 'archive', trigger the archive job first
                    self.rpc.archive(anomaly)
                result = {'message': 'Anomaly updated'}
            elif request.get("copy"):
                aid = self.db.duplicate(session, anomaly, request["copy"])
                result = {'anomalyId': aid}
            session.commit()
        return result

    def _getLogFile(self, anomaly, logfile_id):
        for logfile in anomaly.logfiles:
            if logfile.id == int(logfile_id):
                return logfile
        raise cherrypy.HTTPError(404)

    @cherrypy.expose
    @cherrypy.config(**{'tools.cors.on': True})
    @cherrypy.tools.json_in()
    @cherrypy.tools.json_out(content_type='application/json; charset=utf-8')
    def update_scores(self, anomaly_id, logfile_id):
        """Update a logfile scores"""
        self.log.info("Updating %s logfile %s" % (anomaly_id, logfile_id))
        scores = dict(cherrypy.request.json)
        with self.db.session() as session:
            anomaly = self._getAnomaly(session, anomaly_id)
            to_update = self._getLogFile(anomaly, logfile_id)
            to_delete = []
            for line in to_update.lines:
                if line.nr in scores:
                    line.confidence = scores[line.nr]
                else:
                    to_delete.append(line)
            for deleted_line in to_delete:
                to_update.lines.remove(deleted_line)
                session.delete(deleted_line)
            anomaly.status = "reviewed"
            session.commit()
            return {'msg': 'Logfile %s updated' % (to_update.path)}

    @cherrypy.expose
    @cherrypy.config(**{'tools.cors.on': True})
    @cherrypy.tools.json_out(content_type='application/json; charset=utf-8')
    def delete_logfile(self, anomaly_id, logfile_id):
        self.log.info("Deleting %s logfile %s" % (anomaly_id, logfile_id))
        with self.db.session() as session:
            anomaly = self._getAnomaly(session, anomaly_id)
            to_delete = self._getLogFile(anomaly, logfile_id)
            anomaly.logfiles.remove(to_delete)
            anomaly.status = "reviewed"
            # todo: delete logfile if it's no longer referenced
            # session.delete(to_delete)
            session.commit()
            return {'msg': 'Logfile %s removed' % (to_delete.path)}


class StaticHandler(object):
    """Special Api object to handle static files and html5 path"""
    def __init__(self, root):
        self.root = root

    def default(self, path):
        # Try to handle static file first
        handled = cherrypy.lib.static.staticdir(
            section="",
            dir=self.root,
            index='index.html')
        if not handled:
            # When not found, serve the index.html
            return cherrypy.lib.static.serve_file(
                path=os.path.join(self.root, "index.html"),
                content_type="text/html")


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

    def process(self, request):
        """When user create a report, start a thread to wait for results"""
        if request["uuid"] in self.jobs:
            raise RuntimeError("Job already running")
        # Keep track of the job for the status page
        self.jobs[request["uuid"]] = request
        threading.Thread(target=self.do_process, args=(request,)).start()

    def do_process(self, request):
        """Local thread to wait for worker function and import result in db"""
        self.log.info("DoProcess called with %s", request)
        # The handle_process is implemented by a worker.
        # The result object is similar to the Api.report() input.
        result = self.submitJob("process", {"request": request}, wait=True)

        # Job is no longer running, we can delete its reference
        del self.jobs[request["uuid"]]
        if result.get("report"):
            self.log.info("%s: injesting report" % request["uuid"])
            try:
                with self.api.db.session() as session:
                    aid = self.api.db.import_report(session, result["report"])
                self.history.appendleft("%s: new report added: %s" % (
                    request["uuid"], aid))
            except Exception as e:
                msg = "%s: import failed (%s)" % (request["uuid"], e)
                self.history.appendleft(msg)
                self.log.exception(msg)
        else:
            msg = "%s: %s" % (request["uuid"], result)
            self.history.appendleft(msg)
            self.log.error(msg)

    def archive(self, anomaly):
        """Convert the anomaly object into a dataset object and wait for job"""
        if anomaly.uuid in self.jobs:
            raise RuntimeError("Job already running")
        logfiles = []

        def resolvePath(build, obj):
            urlsplit = urllib.parse.urlsplit(build.log_url)
            return os.path.join(
                self.api.logserver_folder,
                urlsplit.netloc,
                urlsplit.path[1:],
                obj.path)

        baseBuilds = {}
        for logfile in anomaly.logfiles:
            baselines = []
            for baseline in logfile.model.baselines:
                if baseline.build.uuid not in baseBuilds:
                    baseBuilds[baseline.build.uuid] = baseline.build.toDict()
                baselines.append({
                    'local_path': resolvePath(baseline.build, baseline)
                })
            logfiles.append({
                'name': logfile.path.replace('/', '_'),
                'local_path': resolvePath(anomaly.build, logfile),
                'scores': [[l.nr, l.confidence] for l in logfile.lines],
                'baselines': baselines,
            })

        archive = {
            "infos": {
                "uuid": anomaly.uuid,
                "name": anomaly.name,
                "date": time.strftime("%Y-%m-%dT%H:%M:%S"),
                "reporter": anomaly.reporter,
                "build": anomaly.build.toDict(),
                "baselines": list(baseBuilds.values()),
            },
            "logfiles": logfiles,
        }

        # Keep track of the job for the status page
        self.jobs[anomaly.uuid] = "Archiving..."
        threading.Thread(target=self.do_archive, args=(archive,)).start()

    def do_archive(self, archive):
        """Local thread to wait for worker function and record result"""
        uuid = archive["infos"]["uuid"]
        self.log.info("DoArchive began for %s" % uuid)
        try:
            result = self.submitJob("archive", archive, wait=True)
        except RuntimeError:
            return
        finally:
            del self.jobs[uuid]
        if not result.get("error"):
            try:
                # Mark the anomaly as archived and record the archive date
                with self.api.db.session() as session:
                    anomaly = session.query(
                        model.Anomaly).filter_by(uuid=uuid).one()
                    anomaly.status = "archived"
                    anomaly.archive_date = datetime.datetime.strptime(
                        result["date"], "%Y-%m-%dT%H:%M:%S")
                    session.commit()
                msg = "%s: archive completed" % uuid
            except Exception as e:
                msg = "%s: archive failed (%s)" % (uuid, e)
                self.log.exception(msg)
            self.history.appendleft(msg)
        else:
            msg = "%s: %s" % (uuid, result)
            self.log.error(msg)
            self.history.appendleft(msg)

    def handle_archive(self, infos, logfiles):
        """Handle archive job: create dirs, dump info and symlink logs"""
        self.log.info("Archiving %s" % infos["uuid"])
        base_dir = os.path.join(
            self.api.dataset_folder, infos["uuid"][:2], infos["uuid"])

        def xsymlink(src, dest):
            if not os.path.exists(dest):
                os.symlink(src, dest)
        for logfile in logfiles:
            folder = os.path.join(base_dir, logfile['name'])
            baseline_folder = os.path.join(folder, "baselines")
            os.makedirs(baseline_folder, 0o755, exist_ok=True)
            json.dump(logfile['scores'], open(
                os.path.join(folder, "scores.json"), "w"))
            xsymlink(logfile['local_path'],
                     os.path.join(folder, "targetlog.txt"))
            idx = 1
            for baseline in logfile["baselines"]:
                xsymlink(baseline['local_path'],
                         os.path.join(baseline_folder, "%03d.txt" % idx))
                idx += 1
        yaml.safe_dump(
            infos,
            open(os.path.join(base_dir, "infos.yaml"), "w"),
            default_flow_style=False)
        self.log.info("Archive completed %s" % infos["uuid"])
        return {"date": infos["date"]}


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
        route_map.connect('api', '/api/anomaly/new',
                          controller=self.api, action='create',
                          conditions=dict(method=["PUT"]))
        route_map.connect('api', '/api/anomaly',
                          controller=self.api, action='report',
                          conditions=dict(method=["PUT"]))
        route_map.connect('api', '/api/anomaly/{anomaly_id}',
                          controller=self.api, action='get',
                          conditions=dict(method=["GET"]))
        route_map.connect('api', '/api/anomaly/{anomaly_id}',
                          controller=self.api, action='delete',
                          conditions=dict(method=["DELETE"]))
        route_map.connect('api', '/api/anomaly/{anomaly_id}',
                          controller=self.api, action='update',
                          conditions=dict(method=["POST"]))
        route_map.connect('api',
                          '/api/anomaly/{anomaly_id}/logfile/{logfile_id}',
                          controller=self.api, action='update_scores',
                          conditions=dict(method=["POST"]))
        route_map.connect('api',
                          '/api/anomaly/{anomaly_id}/logfile/{logfile_id}',
                          controller=self.api, action='delete_logfile',
                          conditions=dict(method=["DELETE"]))
        route_map.connect('api', '/api/anomalies',
                          controller=self.api, action='list',
                          conditions=dict(method=["GET"]))

        # Add fallthrough routes at the end for the static html/js files
        route_map.connect(
            'root_static', '/{path:.*}',
            controller=os.path.join(os.path.dirname(__file__), 'web'),
            action='default')

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
