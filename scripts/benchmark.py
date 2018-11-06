#!/usr/bin/env python3
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

"""Script to benchmark performance by sample size"""

import copy
import os
import random
import time

import numpy as np
import requests

from logreduce.download import ZuulBuilds
from logreduce.models import models

CACHE = os.path.expanduser("~/.cache/logreduce-benchmark")

########################
# Benchmarh parameters #
########################
# Sources
JOB_NAME = "tempest-full"
BRANCH = "master"
# Measure
MODEL = "hashing_nn"
BATCH_COUNT = 42
MAX_JOB_COUNT = 64
BLOCK_SIZE = 512


def download_data(**kwarg):
    os.makedirs(CACHE, exist_ok=True)
    to_download = []
    # Collect logs url
    print("Listing build: ", kwarg)
    for build in ZuulBuilds("https://zuul.openstack.org/api").get(**kwarg):
        to_download.append(
            (os.path.join(build["log_url"], "job-output.txt.gz"),
             os.path.join(CACHE, kwarg["result"], "%s.txt" % build["uuid"])))
    for url, dest in to_download:
        if os.path.isfile(dest):
            continue
        os.makedirs(os.path.dirname(dest), exist_ok=True)
        r = requests.get(url)
        with open(dest, 'wb') as fd:
            for chunk in r.iter_content(chunk_size=4096):
                fd.write(chunk)
        print("Downloaded %s from %s" % (dest, url))


def single_test(file_list, limit=1e6):
    model = models[MODEL]()
    vectors = []
    count = 0
    print("Loading start")
    loading_start_time = time.monotonic()
    while True:
        for line in open(file_list.pop()):
            count += 1
            if count % 1000 == 0:
                print("%7d\r" % count, end="")
            vectors.append(model.process_line(line))
            if count >= limit:
                break
        if count >= limit:
            break
    print("\nLoading took %f" % (time.monotonic() - loading_start_time))
    model.train(vectors)

    # Pick target
    target = open(file_list.pop()).readlines()

    # Measure test times
    test_times = np.zeros(BATCH_COUNT)

    for batch_nr in range(BATCH_COUNT):
        target_samples = copy.copy(target)
        random.shuffle(target_samples)
        target_samples = target_samples[:BLOCK_SIZE]
        target_vectors = []
        for target_sample in target_samples:
            target_vectors.append(model.process_line(target_sample))

        start_time = time.monotonic()
        model.test(target_vectors)
        test_time = time.monotonic() - start_time
        print("batch %02d: %f\r" % (batch_nr, test_time), end='')
        test_times[batch_nr] = time.monotonic() - start_time

    print("\nTesting took %f\n" % np.mean(test_time))
    exit(0)


def benchmark():
    X_success, X_failure = [], []
    # Load build lists
    for dname, lobj in (("SUCCESS", X_success), ("FAILURE", X_failure)):
        dpath = os.path.join(CACHE, dname)
        fnames = os.listdir(dpath) if os.path.isdir(dpath) else []
        if not fnames:
            download_data(
                job=JOB_NAME, branch=BRANCH, count=MAX_JOB_COUNT, result=dname)
        for fname in fnames:
            lobj.append(os.path.join(dpath, fname))
        random.shuffle(lobj)

    # benchmark 1e6 samples model
    # single_test(X_success)

    model_cls = models[MODEL]
    results = []
    sample_count = 0
    vectors = set()
    for sample_job in range(MAX_JOB_COUNT):
        sample_job += 1
        print("Starting sample_job %02d" % sample_job)
        model = model_cls("benchmark%02d" % sample_job)

        # Load samples
        for line in open(X_success.pop()):
            sample_count += 1
            vectors.add(model.process_line(line))
        print("%d samples loaded (%d)" % (len(vectors), sample_count))

        # Train
        model.train(vectors)

        # Pick target
        target = open(X_failure.pop()).readlines()

        # Measure test times
        test_times = np.zeros(BATCH_COUNT)

        for batch_nr in range(BATCH_COUNT):
            target_samples = copy.copy(target)
            random.shuffle(target_samples)
            target_samples = target_samples[:BLOCK_SIZE]
            target_vectors = []
            for target_sample in target_samples:
                target_vectors.append(model.process_line(target_sample))

            start_time = time.monotonic()
            model.test(target_vectors)
            test_time = time.monotonic() - start_time
            print("batch %02d: %f\r" % (batch_nr, test_time), end='')
            test_times[batch_nr] = time.monotonic() - start_time

        print("\nsample_job %02d: %f\n" % (sample_job, np.mean(test_time)))
        results.append([
            sample_job, sample_count, len(vectors), np.mean(test_times)])
    return results


results = benchmark()
csvpath = os.path.join(CACHE, "benchmark.csv")
with open(csvpath, "w") as of:
    # of.write("Jobs,Samples,Vectors,Time\n")
    for result in results:
        of.write("%d,%d,%d,%.4f\n" % tuple(result))

GNUPLOT = """
cat << EOF | gnuplot
set term png size 1280,800
set output '{filename}.png'
set datafile separator ","
set grid
set title '{title}'
set ylabel '{ylabel}'
set xlabel '{xlabel}'
plot '{csvpath}' using {csv_range} notitle
EOF
"""

print(GNUPLOT.format(
    title="Unique vectors per tempest-full jobs",
    filename="vector-per-jobs",
    ylabel="vectors",
    xlabel="jobs",
    csv_range="0:3 with lines lw 2 ",
    csvpath=csvpath,
    ))

print(GNUPLOT.format(
    title="Search time per sample size",
    filename="time-per-samples",
    ylabel="512 vectors search time (seconds)",
    xlabel="samples",
    csv_range="3:4 with linespoints lw 2",
    csvpath=csvpath,
    ))
