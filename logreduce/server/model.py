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

import alembic
import alembic.command
import alembic.config
import sqlalchemy as sa
import sqlalchemy.pool
from sqlalchemy.ext.declarative import declarative_base
from sqlalchemy.orm import scoped_session, sessionmaker
from contextlib import contextmanager

Base = declarative_base(metadata=sa.MetaData())


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
