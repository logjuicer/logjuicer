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
import os
import re
import struct
import time

import numpy as np
import sklearn.utils.validation
import sklearn.externals

from logreduce.models import models
from logreduce.tokenizer import remove_ansible_std_lines_lists
from logreduce.utils import files_iterator
from logreduce.utils import format_speed
from logreduce.utils import open_file


class Classifier:
    log = logging.getLogger("Classifier")
    version = 1

    def __init__(self,
                 model='bag-of-words_nn', exclude_paths=[], exclude_files=[]):
        self.models = {}
        self.model_name = model
        self.exclude_paths = []
        self.exclude_files = []
        self.test_prefix = None

    def get(self, model_name):
        return self.models.setdefault(model_name,
                                      models[self.model_name](model_name))

    def save(self, fileobj):
        """Save the model"""
        if isinstance(fileobj, str):
            os.makedirs(os.path.dirname(fileobj), 0o700, exist_ok=True)
            fileobj = open(fileobj, 'wb')
        fileobj.write(b'LGRD')
        fileobj.write(struct.pack('I', self.version))
        sklearn.externals.joblib.dump(self, fileobj, compress=True)
        self.log.debug("%s: written" % fileobj.name)

    @staticmethod
    def check(fileobj):
        hdr = fileobj.read(4)
        if hdr != b'LGRD':
            raise RuntimeError("Invalid header")
        version = struct.unpack('I', fileobj.read(4))[0]
        if version != Classifier.version:
            raise RuntimeError("Invalid version")

    @staticmethod
    def load(fileobj):
        """Load a saved model"""
        if isinstance(fileobj, str):
            fileobj = open(fileobj, 'rb')
        Classifier.check(fileobj)
        return sklearn.externals.joblib.load(fileobj)

    @staticmethod
    def filename2modelname(filename):
        """Create a modelname based on filename"""
        # Special case for job-output which is stored at top-level
        if filename.startswith("job-output.txt"):
            return "job-output.txt"
        # Only keep parent directory and first component of the basename
        shortfilename = os.path.join(
            re.subn(r'[a-z0-9]*[0-9][a-z0-9]*[^\s\/-]*', "", os.path.basename(
                os.path.dirname(filename)))[0],
            os.path.basename(filename).split('.')[0])
        # Detect pipeline in path and add job name
        for pipeline in ("check", "gate", "post", "periodic"):
            pipedir = "/%s/" % pipeline
            if pipedir in filename:
                job_name = filename.split(pipedir)[-1].split('/')[0]
                shortfilename = os.path.join(job_name, shortfilename)
                break
        if shortfilename == '':
            # Reduction was too agressive, just keep the filename in this case
            shortfilename = os.path.basename(filename).split('.')[0]
        # Append relevant extensions
        for ext in (".conf", ".audit", ".yaml", ".orig", ".log",
                    ".xml", ".html", ".txt", ".py", ".json", ".yml"):
            if ext in filename:
                shortfilename += ext
        # Remove numbers and symbols
        return re.subn(r'[^a-zA-Z\/\._-]*', '', shortfilename)[0]

    def train(self, path, url_prefixes={}):
        """Train the model"""
        start_time = time.monotonic()
        self.training_lines_count = 0
        self.training_size = 0
        self.baseline = path

        # Group similar files for the same model
        to_train = {}
        for filename, filename_rel in files_iterator(path):
            if [True for ign in self.exclude_files
                    if re.match(ign, os.path.basename(filename))]:
                continue
            if [True for ign in self.exclude_paths
                    if re.search(ign, filename_rel)]:
                continue
            model_name = Classifier.filename2modelname(filename_rel)
            to_train.setdefault(model_name, []).append(filename)

        # Train each model
        for model_name, filenames in to_train.items():
            model_start_time = time.monotonic()
            model = self.get(model_name)
            model.size = 0
            model.count = 0
            # Tokenize and store all lines in train_data
            train_data = []
            for filename in filenames:
                self.log.debug("%s: Loading %s" % (model_name, filename))
                fobj = None
                try:
                    fobj = open_file(filename)
                    idx = 0
                    while True:
                        line = fobj.readline()
                        if line == b'':
                            break
                        line = line.decode('ascii', errors='ignore')
                        # Special case to not train ourself
                        if model_name == "job-output.txt" and (
                                "TASK [log-classify " in line or
                                "TASK [Generate ara report]" in line):
                            break
                        # Remove ansible std_lines list now
                        line = remove_ansible_std_lines_lists(line)
                        for sub_line in line.split(r'\r'):
                            sub_line = model.process_line(sub_line)
                            if sub_line:
                                train_data.append(sub_line)
                        idx += 1
                    model.size += os.stat(filename).st_size
                    model.count += idx
                except KeyboardInterrupt:
                    exit(1)
                except Exception:
                    self.log.exception("%s: couldn't read" % filename)
                    continue
                finally:
                    if fobj:
                        fobj.close()
                # Set forig for report.html absolute url
                forig = filename
                for prefix, url in url_prefixes.items():
                    if filename.startswith(prefix):
                        forig = os.path.join(url,
                                             filename[len(prefix):])
                        break
                model.sources.append(forig)

            if not train_data:
                self.log.info("%s: no training data found" % model_name)
                continue

            self.training_lines_count += model.count
            self.training_size += model.size
            try:
                # Transform and fit the model data
                model = self.get(model_name)
                model.train(train_data)
                model.train_time = time.monotonic() - model_start_time

                self.log.debug("%s: %s %s" % (
                    model_name, model.info,
                    format_speed(model.count, model.size, model.train_time)))
            except ValueError:
                self.log.exception("%s: couldn't train with %s" % (model_name,
                                                                   train_data))
                del self.models[model_name]
            except KeyboardInterrupt:
                exit(1)
            except Exception:
                self.log.exception("%s: couldn't train with %s" % (model_name,
                                                                   train_data))
                del self.models[model_name]
        self.train_time = time.monotonic() - start_time
        self.train_size_speed = (
            self.training_size / (1024 * 1024)) / self.train_time
        self.train_count_speed = (
            self.training_lines_count / 1000) / self.train_time
        self.log.info(
            "Training took %.03f seconds to ingest %.03f MB "
            "(%.03f MB/s) or %d lines (%.03f kl/s)" % (
                self.train_time,
                self.training_size / (1024 * 1024),
                self.train_size_speed,
                self.training_lines_count,
                self.train_count_speed))
        if not self.training_lines_count:
            raise RuntimeError("No train lines found")
        return self.training_lines_count


#    @profile
    def test(self, path):
        """Return outliers"""
        start_time = time.monotonic()
        self.testing_lines_count = 0
        self.testing_size = 0
        self.outlier_lines_count = 0

        for filename, filename_rel in files_iterator(path):
            if [True for ign in self.exclude_files
                    if re.match(ign, os.path.basename(filename))]:
                continue
            if [True for ign in self.exclude_paths
                    if re.search(ign, filename_rel)]:
                continue
            test_start_time = time.monotonic()

            if self.test_prefix:
                filename_rel = os.path.join(self.test_prefix, filename_rel)

            # Set filename_orig for report.html relative url
            if filename_rel.startswith("job-output.txt"):
                filename_orig = "job-output.txt.gz"
            else:
                filename_orig = filename_rel
            if len(self.models) > 1:
                # Get model name based on filename
                model_name = Classifier.filename2modelname(filename_rel)
                if model_name not in self.models:
                    self.log.debug("Skipping unknown file %s (%s)" % (
                        filename, model_name))
                    yield (filename_rel, filename_orig, None, None, None)
                    continue
            else:
                # Only one file was trained, use its model
                model_name = list(self.models.keys())[0]
            self.log.debug("%s: Testing %s" % (model_name, filename))

            # Store file line number in test_data_pos
            data = []
            test_data_pos = []
            # Tokenize and store all lines in test_data
            test_data = []
            test_data_set = set()
            model = self.get(model_name)

            fobj = None
            try:
                fobj = open_file(filename)
                idx = 0
                while True:
                    line = fobj.readline()
                    if line == b'':
                        break
                    line = line.decode('ascii', errors='ignore')
                    # Special case to not test ourself
                    if model_name == "job-output.txt" and (
                            "TASK [log-classify " in line or
                            "TASK [Generate ara report]" in line):
                        break
                    # Remove ansible std_lines list now
                    line = remove_ansible_std_lines_lists(line)
                    data.append(line)
                    for sub_line in line.split(r'\r'):
                        sub_line = model.process_line(sub_line)
                        if sub_line and sub_line not in test_data_set:
                            test_data.append(sub_line)
                            test_data_set.add(sub_line)
                            test_data_pos.append(idx)
                    idx += 1
                del test_data_set
                self.testing_size += os.stat(filename).st_size
                self.testing_lines_count += idx
            except KeyboardInterrupt:
                exit(1)
            except Exception:
                self.log.exception("%s: couldn't read" % filename)
                continue
            finally:
                if fobj:
                    fobj.close()

            if not test_data:
                self.log.warning("%s: not valid lines" % filename)
                continue

            # Transform and compute distance from the model
            model = self.models[model_name]
            try:
                distances = model.test(test_data)
            except sklearn.utils.validation.NotFittedError:
                self.log.warning("%s: skipping unfitted model" % filename)
                continue

            def get_line_info(line_pos):
                try:
                    distance = distances[test_data_pos.index(line_pos)]
                except ValueError:
                    # Line wasn't in test data
                    distance = 0.0
                return (line_pos, distance, data[line_pos])

            # Store (line_pos, distance, line) in outliers
            outliers = []
            last_outlier = 0
            remaining_after_context = 0
            line_pos = 0
            while line_pos < len(data):
                line_pos, distance, line = get_line_info(line_pos)
                if distance >= self.threshold:
                    if line_pos - last_outlier >= self.merge_distance:
                        # When last outlier is too far,
                        # set last_outlier to before_context
                        last_outlier = max(line_pos - 1 - self.before_context,
                                           -1)
                    # Add previous context
                    for prev_pos in range(last_outlier + 1, line_pos):
                        outliers.append(get_line_info(prev_pos))
                    last_outlier = line_pos

                    outliers.append((line_pos, distance, line))
                    self.outlier_lines_count += 1
                    remaining_after_context = self.after_context
                elif remaining_after_context > 0:
                    outliers.append((line_pos, distance, line))
                    remaining_after_context -= 1
                    last_outlier = line_pos
                line_pos += 1

            # Yield result
            yield (filename_rel, filename_orig, model, outliers,
                   time.monotonic() - test_start_time)

        self.test_time = time.monotonic() - start_time
        self.log.info("Testing took %s" % format_speed(
            self.testing_lines_count, self.testing_size, self.test_time))
        if not self.testing_lines_count:
            raise RuntimeError("No test lines found")

    def process(self, path, path_source=None, threshold=0.2, merge_distance=5,
                before_context=3, after_context=1, console_output=False):
        """Process target and create a report"""
        self.threshold = threshold
        self.merge_distance = merge_distance
        self.before_context = before_context
        self.after_context = after_context
        output = {'files': {}, 'unknown_files': [], 'models': {},
                  'anomalies_count': 0}
        for file_result in self.test(path):
            filename, filename_orig, model, outliers, test_time = file_result
            if model is None:
                # Do not bother with failed only logfile
                if "failed_deployment_list.log.txt" not in filename:
                    output["unknown_files"].append((filename, filename_orig))
                continue
            if path_source is not None:
                filename_orig = os.path.join(path_source, filename_orig)
            output['models'].setdefault(model.name, {
                'source_files': model.sources,
                'train_time': model.train_time,
                'info': model.info,
            })
            file_info = output['files'].setdefault(filename, {
                'file_url': filename_orig,
                'test_time': test_time,
                'model': model.name,
                'chunks': [],
                'scores': [],
                'line_pos': [],
                'lines_count': 0,
            })
            current_chunk = []
            current_score = []
            current_pos = []
            last_pos = None
            self.log.debug("%s: compared with %s" % (
                filename, " ".join(model.sources)))

            for pos, distance, outlier in outliers:
                distance = abs(float(distance))
                if last_pos and pos - last_pos != 1:
                    # New chunk
                    file_info["chunks"].append("\n".join(current_chunk))
                    file_info["scores"].append(current_score)
                    file_info["line_pos"].append(current_pos)
                    file_info["lines_count"] += len(current_chunk)
                    current_chunk = []
                    current_score = []
                    current_pos = []
                    if last_pos and console_output:
                        print()

                # Clean ansible one-liner outputs
                for line in outlier[:-1].split(r'\n'):
                    line = line.replace(r'\t', '\t')
                    current_score.append(distance)
                    current_chunk.append(line)
                    current_pos.append(pos)
                    if console_output:
                        print("%1.3f | %s:%04d:\t%s" % (distance,
                                                        filename,
                                                        pos + 1,
                                                        line))

                last_pos = pos
            if current_chunk:
                file_info["chunks"].append("\n".join(current_chunk))
                file_info["scores"].append(current_score)
                file_info["line_pos"].append(current_pos)
                file_info["lines_count"] += len(current_chunk)

            # Compute mean distances of outliers
            mean_distance = 0
            if file_info["scores"]:
                mean_distance = np.mean(np.hstack(file_info["scores"]))
                output["anomalies_count"] += len(file_info["scores"])
            file_info["mean_distance"] = mean_distance

        output["training_lines_count"] = self.training_lines_count
        output["testing_lines_count"] = self.testing_lines_count
        output["outlier_lines_count"] = self.outlier_lines_count
        output["reduction"] = 100 - (output["outlier_lines_count"] /
                                     output["testing_lines_count"]) * 100
        output["baseline"] = self.baseline
        output["target"] = [path] if isinstance(path, str) else path
        return output
