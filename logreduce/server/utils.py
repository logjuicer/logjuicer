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


def formatTime(dt):
    if dt:
        return dt.strftime("%Y-%m-%dT%H:%M:%S")
    else:
        return ''


def usage(service_name):
    """Handle CLI usage and setup logging"""
    import argparse
    import yaml
    import logging.config

    parser = argparse.ArgumentParser(description="%s daemon" % service_name)
    parser.add_argument("-c", required=True, dest='config',
                        help="configuration file path")
    parser.add_argument("--debug", action="store_true",
                        help="unable debug log")
    args = parser.parse_args()
    config = yaml.safe_load(open(args.config))

    log = config["logging"]
    log["version"] = 1
    log["handlers"]["file"]["filename"] = log["handlers"]["file"][
        "filename"].format(service=service_name)
    if args.debug:
        log["loggers"]["logreduce"]["level"] = "DEBUG"
        log["root"]["level"] = "DEBUG"
        log["root"]["handlers"] = ['console']
        for logger in log["loggers"].values():
            logger['handlers'] = ['console']
        del log['handlers']['file']
        log['handlers']['console']['level'] = 'DEBUG'
    logging.config.dictConfig(log)

    return config


def run(services):
    """Handle daemon loop start and stop on SIGTERM and KeyboardInterrupt"""
    import signal

    def exitHandler(sig, val):
        for service in services:
            service.stop()
        exit(0)

    signal.signal(signal.SIGTERM, exitHandler)

    for service in services:
        service.start()

    while True:
        try:
            signal.pause()
        except KeyboardInterrupt:
            print("Ctrl + C: asking scheduler to exit nicely...\n")
            exitHandler(signal.SIGINT, None)
