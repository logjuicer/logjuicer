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

import copy
import datetime
import logging
import uuid
import os

import numpy as np
import voluptuous as v

import alembic
import alembic.command
import alembic.config
import sqlalchemy as sa
import sqlalchemy.pool
from sqlalchemy.ext.declarative import declarative_base
from sqlalchemy.orm import relationship
from sqlalchemy.orm import scoped_session, sessionmaker
from contextlib import contextmanager

import logreduce.server.utils as utils

Base = declarative_base(metadata=sa.MetaData())


################
# Database ORM #
################
class Build(Base):
    """Zuul Builds cache to link ref_url associated with logfiles"""
    __tablename__ = "build"
    uuid = sa.Column(sa.String(36), primary_key=True)
    result = sa.Column(sa.String(255))
    branch = sa.Column(sa.String(255))
    pipeline = sa.Column(sa.String(255))
    ref_url = sa.Column(sa.String(255))
    ref = sa.Column(sa.String(255))
    job = sa.Column(sa.String(255))
    project = sa.Column(sa.String(255))
    log_url = sa.Column(sa.String(255))
    end_time = sa.Column(sa.DateTime())

    def toDict(self):
        d = copy.copy(self.__dict__)
        del d['_sa_instance_state']
        if d["end_time"]:
            d["end_time"] = d["end_time"].strftime("%Y-%m-%dT%H:%M:%S")
        return d


class Baseline(Base):
    """The list of file used to train a model"""
    __tablename__ = "baseline"
    id = sa.Column(sa.Integer, primary_key=True)
    path = sa.Column(sa.String(512))
    model_uuid = sa.Column(sa.Integer, sa.ForeignKey("model.uuid"))
    build_uuid = sa.Column(sa.String(36), sa.ForeignKey("build.uuid"))

    build = relationship(Build, lazy="subquery")


class Model(Base):
    """The list of model used to analyze files"""
    __tablename__ = "model"
    uuid = sa.Column(sa.String(36), primary_key=True)
    name = sa.Column(sa.String(255), primary_key=True)
    train_time = sa.Column(sa.Float)
    info = sa.Column(sa.String(255))

    baselines = relationship(Baseline, lazy="subquery")


class Line(Base):
    """The confidence score for file's lines"""
    __tablename__ = "line"
    id = sa.Column(sa.Integer, primary_key=True)
    logfile_id = sa.Column(sa.Integer, sa.ForeignKey("logfile.id"))
    nr = sa.Column(sa.Integer)
    confidence = sa.Column(sa.Float)


class LogFile(Base):
    """A logfile containing anomalies"""
    __tablename__ = "logfile"
    id = sa.Column(sa.Integer, primary_key=True)
    anomaly_uuid = sa.Column(sa.Integer, sa.ForeignKey("anomaly.uuid"))
    model_uuid = sa.Column(sa.String, sa.ForeignKey("model.uuid"))
    path = sa.Column(sa.String(255))
    test_time = sa.Column(sa.Float)

    lines = relationship(Line, lazy="subquery")
    model = relationship(Model, lazy="subquery")


class Anomaly(Base):
    __tablename__ = "anomaly"
    uuid = sa.Column(sa.String(36), primary_key=True)
    name = sa.Column(sa.String(255))
    reporter = sa.Column(sa.String(255))
    status = sa.Column(sa.Enum('processed', 'reviewed', 'archived'))
    report_date = sa.Column(sa.DateTime())
    archive_date = sa.Column(sa.DateTime())
    test_command = sa.Column(sa.String(512))
    train_command = sa.Column(sa.String(512))
    build_uuid = sa.Column(sa.String(36), sa.ForeignKey("build.uuid"))

    build = relationship(Build, lazy="subquery")
    logfiles = relationship(LogFile, lazy="subquery")


#################
# Input Schemas #
#################
class Report:
    build = v.Schema({
        'uuid': str,
        'log_url': str,
        'result': str,
        'branch': str,
        'pipeline': str,
        'ref_url': str,
        'ref': str,
        'job': str,
        'project': str,
        'end_time': str
    }, extra=v.REMOVE_EXTRA)

    log = {
        v.Required('path'): str,
        'model': str,
        'scores': [[int, float]],
        'test_time': float,
    }

    model = {
        v.Required('name'): str,
        'uuid': str,
        'source_files': [str],
        'train_time': float,
        'info': str,
    }

    report = {
        'name': str,
        'reporter': str,
        'target': build,
        'baselines': [build],
        'models': [model],
        'logs': [log],
        'test_command': str,
        'train_command': str,
    }

    schema = v.Schema(report)


class UserReport:
    report = {
        'name': v.Required(str),
        # Uuid is a Zuul build id
        'uuid': v.Required(str),
        'reporter': v.Required(str),
        # Url is the zuul api url
        'url': str,
        # Path is an optional --include-path parameter
        'path': str,
        # logpath and lines are used when receiving report from os-loganalyze
        'logpath': str,
        'lines': (int, int),
    }

    schema = v.Schema(report)


def parse_time(build):
    if build["end_time"]:
        build["end_time"] = datetime.datetime.strptime(
            build["end_time"], "%Y-%m-%dT%H:%M:%S")
    else:
        del build["end_time"]


class Db(object):
    log = logging.getLogger("logreduce.DB")

    def __init__(self, dburi="sqlite:///logreduce.sqlite"):
        self.dburi = dburi
        # Create database and run migration script
        engine = self.createEngine()
        Base.metadata.create_all(engine)
        self._migrate(engine)
        engine.dispose()

    def createEngine(self):
        engine_args = {}
        if not self.dburi.startswith("sqlite://"):
            engine_args['poolclass'] = sqlalchemy.pool.QueuePool
            engine_args['pool_recycle'] = 1
        return sa.create_engine(self.dburi, **engine_args)
        self.connected = True

    def session(self):
        engine = self.createEngine()
        Base.metadata.create_all(engine)
        session = scoped_session(sessionmaker(bind=engine))

        @contextmanager
        def autoclose():
            yield session
            session.remove()
        return autoclose()

    def duplicate(self, session, old_anomaly, copyInfo):
        anomaly = Anomaly(
            uuid=str(uuid.uuid4()),
            name=old_anomaly.name,
            reporter=old_anomaly.reporter,
            status='reviewed',
            report_date=old_anomaly.report_date,
            archive_date=old_anomaly.archive_date,
            test_command=old_anomaly.test_command,
            train_command=old_anomaly.train_command,
            build=old_anomaly.build
        )
        if copyInfo.get("name"):
            anomaly.name = copyInfo["name"]
        if copyInfo.get("reporter"):
            anomaly.reporter = copyInfo["reporter"]
        logfileFilter = copyInfo.get('logfiles', None)
        for logfile in old_anomaly.logfiles:
            if logfileFilter and logfile.id not in logfileFilter:
                continue
            anomaly.logfiles.append(LogFile(
                model=logfile.model,
                path=logfile.path,
                test_time=logfile.test_time,
                lines=list(map(
                    lambda x: Line(nr=x.nr, confidence=x.confidence),
                    logfile.lines))))
        session.add(anomaly)
        return anomaly.uuid

    def export(self, anomaly):
        baselines = {}
        logfiles = []
        models = {}
        for logfile in anomaly.logfiles:
            scores = []
            for line in logfile.lines:
                scores.append([line.nr, line.confidence])
            # Get model info
            model = logfile.model
            if model.name not in models:
                modelfiles = []
                modelbuilds = set()
                for baseline in model.baselines:
                    modelfiles.append(os.path.join(
                        baseline.build.log_url, baseline.path))
                    build = baseline.build.toDict()
                    if build["uuid"] not in baselines:
                        baselines[build["uuid"]] = build
                    if build["uuid"] not in modelbuilds:
                        modelbuilds.add(build["uuid"])
                models[model.name] = {
                    'train_time': model.train_time,
                    'info': model.info,
                    'source_files': modelfiles,
                    'source_builds': list(modelbuilds)
                }
            mean_distance = np.mean(np.array(scores)[:, 1])
            logfiles.append({
                'id': logfile.id,
                'model': logfile.model.name,
                'path': logfile.path,
                'test_time': logfile.test_time,
                'scores': scores,
                'lines': [],
                'mean_distance': mean_distance
            })

        return {
            'uuid': anomaly.uuid,
            'name': anomaly.name,
            'status': anomaly.status,
            'test_command': anomaly.test_command,
            'train_command': anomaly.train_command,
            'reporter': anomaly.reporter,
            'report_date': utils.formatTime(anomaly.report_date),
            'archive_date': utils.formatTime(anomaly.archive_date),
            'build': anomaly.build.toDict(),
            'baselines': list(baselines.values()),
            'logfiles': sorted(
                logfiles,
                key=lambda x: (x['path'].startswith("job-output.txt") or
                               x['mean_distance']),
                reverse=True),
            'models': models,
        }

    def import_report(self, session, report):
        """Import a logreduce report"""
        self.log.info("Importing report start")
        report = Report.schema(report)
        anomaly_uuid = str(uuid.uuid4())

        # Insert build and models
        target = session.query(Build).get(report["target"]["uuid"])
        if not target:
            parse_time(report['target'])
            target = Build(**report["target"])
        # extract baselines and insert build from the baseline list
        baselines = {}
        for baseline in report["baselines"]:
            baselines[baseline["log_url"]] = session.query(Build).get(
                baseline["uuid"])
            if not baselines[baseline["log_url"]]:
                parse_time(baseline)
                baselines[baseline["log_url"]] = Build(**baseline)

        # extract models and insert model used in logfiles
        models = {}
        for model in report["models"]:
            models[model["name"]] = session.query(Model).get(
                (model["uuid"], model["name"]))
            if not models[model["name"]]:
                baseline_files = []
                for source_file in model["source_files"]:
                    build = None
                    for log_url, baseline in baselines.items():
                        if source_file.startswith(log_url):
                            build = baseline
                            baseline_path = source_file[len(log_url):]
                            break
                    baseline_files.append(Baseline(
                        path=baseline_path, build=build))
                model["baselines"] = baseline_files
                del model["source_files"]
                models[model["name"]] = Model(**model)

        # create logfile objects
        logfiles = []
        for log in report["logs"]:
            log["model"] = models[log["model"]]
            log["lines"] = list(map(
                lambda x: Line(nr=x[0], confidence=x[1]), log["scores"]))
            del log["scores"]
            logfiles.append(LogFile(**log))

        # Create anomaly entry
        anomaly = Anomaly(
            uuid=anomaly_uuid,
            name=report['name'],
            reporter=report['reporter'],
            status='processed',
            build=target,
            train_command=report.get('train_command'),
            test_command=report.get('test_command'),
            logfiles=logfiles,
            report_date=datetime.datetime.utcnow(),
        )
        session.add(anomaly)
        session.commit()
        self.log.info("Importing report done: %s", anomaly_uuid)
        return anomaly_uuid

    def _migrate(self, engine):
        """Perform the alembic migrations"""
        with engine.begin() as conn:
            context = alembic.migration.MigrationContext.configure(conn)
            current_rev = context.get_current_revision()
            self.log.debug('Current migration revision: %s' % current_rev)

            config = alembic.config.Config()
            config.set_main_option("script_location",
                                   "logreduce:server/sql/alembic")
            config.set_main_option("sqlalchemy.url", self.dburi)
            alembic.command.upgrade(config, 'head')
