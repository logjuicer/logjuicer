# Copyright (c) 2017 Red Hat
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

import argparse
import asyncio
import json
import logging
import re
import os
from urllib.parse import urlparse
import urllib.request

import aiohttp


def usage():
    p = argparse.ArgumentParser(description="Recursive http download")
    p.add_argument("url", nargs='?')
    p.add_argument("--exclude-file", action="append", default=[],
                   help="File object regexp to exclude")
    p.add_argument("--exclude-path", action="append", default=[],
                   help="Path regexp to exclude")
    p.add_argument("--exclude-extension", action="append", default=[],
                   help="Extension list to exclude")
    p.add_argument("--dest", default=os.getcwd(),
                   help="Save files to this directory")
    p.add_argument("--threads", default=4, type=int,
                   help="Number of parallel download")
    p.add_argument("--zuul-url", default="http://zuul.openstack.org",
                   help="The zuul url to download similar job results")
    p.add_argument("--job")
    p.add_argument("--count", type=int, default=3)
    p.add_argument("--pipeline", default="gate")
    p.add_argument("--project")
    p.add_argument("--verbose", action="store_true",
                   help="Print each file download")
    args = p.parse_args()
    if not args.url and not args.job:
        print("Direct url or job name is required")
        exit(1)
    logging.basicConfig(
        format='%(asctime)s %(levelname)-5.5s %(name)s - %(message)s',
        level=logging.DEBUG if args.verbose else logging.INFO)
    return args


class RecursiveDownload:
    log = logging.getLogger("logreduce.RecursiveDownload")

    def __init__(self, url, dest, threads=4, trim=None,
                 exclude_files=[], exclude_paths=[], exclude_extensions=[]):
        self.url = url
        self.dest = dest
        self.exclude_files = exclude_files
        self.exclude_paths = exclude_paths
        self.exclude_extensions = exclude_extensions
        self.active_worker = 0
        self.trim = trim

        try:
            loop = asyncio.get_event_loop()
        except RuntimeError:
            loop = asyncio.new_event_loop()
            asyncio.set_event_loop(loop)

        self.queue = asyncio.Queue()
        self.queue.put_nowait(url)
        self.tasks = [loop.create_task(self.handle_task(idx))
                      for idx in range(threads)]

    def wait(self):
        loop = asyncio.get_event_loop()
        loop.run_until_complete(asyncio.wait(self.tasks))
        return self.get_local_path(self.url)

    def get_local_path(self, url):
        '''Convert url to local path'''
        if self.trim:
            dest = os.path.join(self.dest, url.replace(self.trim, ''))
        else:
            u = urlparse(url)
            dest = os.path.join(self.dest, u.netloc, u.path[1:])
        return dest

    async def list_dir(self, worker_id, url):
        '''Return list of directory element'''
        async with aiohttp.ClientSession() as session:
            self.log.debug('Listing %s', url)
            async with session.get(url, timeout=30) as response:
                assert response.status == 200
                html = await response.read()
                urls = []
                for line in html.decode('utf-8').split('\n'):
                    m = re.match(r'.*<a href="([a-zA-Z0-9][^"]+)">',
                                 line)
                    if m:
                        obj_name = m.groups()[0]
                        if [True for skip in self.exclude_extensions if
                                obj_name.endswith("%s" % skip) or
                                obj_name.endswith("%s.gz" % skip) or
                                obj_name.endswith("%s.bz2" % skip) or
                                obj_name.endswith("%s.xz" % skip)]:
                            continue

                        if [True for ign in self.exclude_files
                                if re.match(ign, obj_name)]:
                            continue
                        curl = "%s%s" % (url, obj_name)
                        if [True for ign in self.exclude_paths
                                if re.search(ign, curl)]:
                            continue
                        urls.append(curl)
                return urls

    async def get_file(self, worker_id, url):
        '''Download a single file'''
        local_path = self.get_local_path(url)
        if os.path.exists(local_path) and os.stat(local_path).st_size > 0:
            # Already downloaded
            self.log.debug('Skipping already downloaded %s', local_path)
            return
        async with aiohttp.ClientSession() as session:
            self.log.debug('Getting %s to %s', url, local_path)
            async with session.get(url, timeout=30) as response:
                assert response.status == 200
                data = await response.read()
                os.makedirs(os.path.dirname(local_path), 0o755, exist_ok=True)
                with open(local_path, "wb") as of:
                    of.write(data)

    async def handle_task(self, worker_id):
        '''For each queue item, list directory or download file'''
        while True:
            try:
                queue_url = self.queue.get_nowait()
            except asyncio.QueueEmpty:
                await asyncio.sleep(0.1)
                if self.active_worker == 0:
                    break
                continue
            self.active_worker += 1
            try:
                if queue_url[-1] == "/":
                    for url in await self.list_dir(worker_id, queue_url):
                        self.queue.put_nowait(url)
                else:
                    await self.get_file(worker_id, queue_url)
            except Exception as e:
                logging.exception("%s: error (%s)" % (queue_url, e))
            self.active_worker -= 1


class ZuulBuild(dict):
    def __repr__(self):
        inf = "id=%s ref=%s" % (self['uuid'][:7], self['ref'])
        if self.get("project"):
            inf += " project=%s" % self['project']
        if self.get('local_path'):
            inf += " local_path=%s" % self['local_path']
        if self.get("log_url"):
            inf += " log_url=%s" % self['log_url']
        return "<ZuulBuild %s>" % inf

    def __str__(self):
        return self.__repr__()

    def __unicode__(self):
        return self.__repr__()


class ZuulBuilds:
    log = logging.getLogger("logreduce.ZuulBuilds")

    def __init__(self, zuul_url):
        self.zuul_url = zuul_url

    def get(self, job=None, project=None, pipeline=None,
            branch=None, uuid=None,
            count=3, result=None):
        url = "%s/builds" % self.zuul_url
        args = ""
        if job:
            args += "&job_name=%s" % job
        if project:
            args += "&project=%s" % project
        if branch:
            args += "&branch=%s" % branch
        if pipeline:
            args += "&pipeline=%s" % pipeline
        if uuid:
            args += "&uuid=%s" % uuid
        if result:
            args += "&result=%s" % result
        if args:
            url = "%s?%s" % (url, args[1:])
        self.log.debug('Getting %s' % url)
        resp = urllib.request.urlopen(url)
        builds_data = json.loads(resp.read().decode('utf-8'))
        builds = []
        for build in builds_data[:count]:
            # Discover true log_url when success-url is nested
            log_url = build["log_url"].rstrip('/')
            attempts = 5
            while attempts > 0:
                inf_url = os.path.join(log_url, "zuul-info/inventory.yaml")
                self.log.debug('Checking %s' % inf_url)
                req = urllib.request.Request(inf_url, method='HEAD')
                try:
                    resp = urllib.request.urlopen(req)
                    if resp.status == 200:
                        build["log_url"] = "%s/" % log_url
                        break
                except urllib.error.HTTPError:
                    pass
                attempts -= 1
                log_url = os.path.dirname(log_url)
            builds.append(ZuulBuild(build))
        return builds


def main():
    args = usage()
    urls = []
    if args.url:
        urls.append(args.url)
    else:
        for build in ZuulBuilds(args.zuul_url).get(
                job=args.job,
                pipeline=args.pipeline,
                project=args.project,
                count=args.count):
            urls.append(build['log_url'])
    for url in urls:
        print(RecursiveDownload(url, args.dest, args.threads,
                                exclude_files=args.exclude_file,
                                exclude_paths=args.exclude_path,
                                exclude_extensions=args.exclude_extension
                                ).wait())


if __name__ == "__main__":
    main()
