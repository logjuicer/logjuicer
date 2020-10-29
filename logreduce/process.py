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
import struct
import sys
import time
import uuid

import numpy as np
import sklearn.utils.validation
import sklearn.exceptions

try:
    from sklearn.externals import joblib
except ImportError:
    # Recent sklearn library doesn't vendor joblib anymore
    import joblib

from typing import (
    List,
    Optional,
    BinaryIO,
    Dict,
    Sequence,
    Tuple,
    Generator,
    Set,
    Callable,
)

from logreduce.data import Result, LogObject
from logreduce.models import Model, models
from logreduce.tokenizer import Tokenizer, filename2modelname
from logreduce.utils import files_iterator
from logreduce.utils import format_speed
from logreduce.utils import open_file

TestResult = Tuple[
    # Filename relative path
    str,
    # Original file uri
    str,
    # The model used containing the associated baseline
    Optional[Model],
    # The list of anomalous line: (line number, distance between 0 and 1, line content)
    Optional[List[Tuple[int, float, str]]],
    # test time
    Optional[float],
]


# The default behavior is to classify all the files
def keep_all(_fn: str) -> bool:
    return True


class Classifier:
    log = logging.getLogger("logreduce.Classifier")
    # Bump this version when models created with earlier versions
    # should be rejected
    version = 8

    def __init__(
        self,
        model="hashing_nn",
        filename_to_modelname: Callable[[str], str] = filename2modelname,
        keep_file: Callable[[str], bool] = keep_all,
        process_line: Callable[[str], str] = Tokenizer.process,
    ):
        self.models: Dict[str, Model] = {}
        self.model_name = model
        self.filename_to_modelname = filename_to_modelname
        self.keep_file = keep_file
        self.process_line = process_line
        self.test_prefix = None
        # Default
        self.threshold = 0.2
        self.merge_distance = 5
        self.before_context = 2
        self.after_context = 2

    def get(self, model_name: str) -> Model:
        return self.models.setdefault(model_name, models[self.model_name](model_name))

    def save_file(self, file_path: str) -> None:
        if os.path.dirname(file_path):
            os.makedirs(os.path.dirname(file_path), 0o700, exist_ok=True)
        self.save(open(file_path, "wb"))

    def save(self, fileobj: BinaryIO) -> None:
        """Save the model"""
        fileobj.write(b"LGRD")
        fileobj.write(struct.pack("I", self.version))
        # Remove functional attributes
        del self.filename_to_modelname
        del self.keep_file
        del self.process_line
        joblib.dump(self, fileobj, compress=True)
        self.log.debug("%s: written" % fileobj.name)

    @staticmethod
    def check(fileobj: BinaryIO) -> None:
        hdr = fileobj.read(4)
        if hdr != b"LGRD":
            raise RuntimeError("Invalid header")
        version = struct.unpack("I", fileobj.read(4))[0]
        if version != Classifier.version:
            raise RuntimeError("Invalid version")

    @staticmethod
    def load_file(
        file_path: str,
        filename_to_modelname: Callable[[str], str] = filename2modelname,
        keep_file: Callable[[str], bool] = keep_all,
        process_line: Callable[[str], str] = Tokenizer.process,
    ) -> "Classifier":
        return Classifier.load(
            open(file_path, "rb"), filename_to_modelname, keep_file, process_line
        )

    @staticmethod
    def load(
        fileobj: BinaryIO,
        filename_to_modelname: Callable[[str], str] = filename2modelname,
        keep_file: Callable[[str], bool] = keep_all,
        process_line: Callable[[str], str] = Tokenizer.process,
    ) -> "Classifier":
        """Load a saved model"""
        Classifier.check(fileobj)
        obj = joblib.load(fileobj)
        obj.keep_file = keep_file
        obj.filename_to_modelname = filename_to_modelname
        obj.process_line = process_line
        return obj

    @staticmethod
    def _is_log_classify_invocation(model_name: str, line: str) -> bool:
        """ Returns True if the line is related to log-classify"""
        return model_name == "job-output.txt" and (
            "TASK [log-classify " in line or "TASK [Generate ara report]" in line
        )

    def train(self, baselines: Sequence[LogObject], command=sys.argv) -> int:
        """Train the model, baselines can be path(s) or build dict(s)"""
        start_time = time.monotonic()
        self.train_command = " ".join(command)
        self.training_lines_count = 0
        self.training_size = 0
        if not len(baselines):
            raise RuntimeError("Empty training baselines")

        self.baselines = baselines

        # Group similar files for the same model
        to_train: Dict[str, List[LogObject]] = {}
        for filename, filename_rel in files_iterator(baselines, self.keep_file):
            model_name = self.filename_to_modelname(filename_rel)
            to_train.setdefault(model_name, []).append(filename)

        # Train each model
        for model_name, filenames in to_train.items():
            model_start_time = time.monotonic()
            model = self.get(model_name)
            model.size = 0
            model.count = 0
            model.uuid = str(uuid.uuid4())
            # Tokenize and store all de-duplicated lines in train_data
            train_data = set()
            for filename in filenames:
                self.log.debug("%s: Loading %s" % (model_name, filename))
                fobj = None
                try:
                    fobj = open_file(filename)
                    for bline in fobj:
                        line = bline.decode("ascii", errors="ignore")
                        # Special case to not train ourself
                        if self._is_log_classify_invocation(model_name, line):
                            break
                        train_data.add(self.process_line(line))
                        model.count += 1
                    try:
                        if isinstance(filename, str):
                            model.size += os.stat(filename).st_size
                    except TypeError:
                        pass
                except UnicodeDecodeError:
                    self.log.info("%s: not a valid utf-8 file", filename)
                except KeyboardInterrupt:
                    exit(1)
                except Exception:
                    self.log.exception("%s: couldn't read" % filename)
                    continue
                finally:
                    if fobj:
                        fobj.close()
                # Check for remote file source location
                forig = filename
                for build in self.baselines:
                    if isinstance(build, dict):
                        build_prefix = "%s/" % build.get("local_path", "").rstrip("/")
                        if isinstance(filename, str) and filename.startswith(
                            build_prefix
                        ):
                            forig = os.path.join(
                                build["log_url"], filename[len(build_prefix) :]
                            )
                            break
                model.sources.append(forig)

            if not train_data:
                self.log.info("%s: no training data found" % model_name)
                continue

            self.training_lines_count += model.count
            self.training_size += model.size
            train_data_time = time.monotonic() - model_start_time
            self.log.debug(
                "%s: Parsing took %s",
                model_name,
                format_speed(model.count, model.size, train_data_time),
            )
            try:
                # Transform and fit the model data
                train_start_time = time.monotonic()
                model = self.get(model_name)
                model.train(train_data)
                model.train_time = time.monotonic() - train_start_time

                self.log.debug(
                    "%s: Fitting took %s"
                    % (
                        model_name,
                        format_speed(model.count, model.size, model.train_time),
                    )
                )
            except ValueError:
                self.log.exception(
                    "%s: couldn't train with %s" % (model_name, train_data)
                )
                del self.models[model_name]
            except KeyboardInterrupt:
                exit(1)
            except Exception:
                self.log.exception(
                    "%s: couldn't train with %s" % (model_name, train_data)
                )
                del self.models[model_name]
        self.train_time = time.monotonic() - start_time
        self.log.info(
            "Training took %s"
            % format_speed(
                self.training_lines_count, self.training_size, self.train_time
            )
        )
        if not self.training_lines_count:
            raise RuntimeError("No train lines found")
        return self.training_lines_count

    #    @profile
    def test(self, targets: Sequence[LogObject]) -> Generator[TestResult, None, None]:
        """Return outliers, target can be path(s) or build dict(s)"""
        start_time = time.monotonic()
        self.testing_lines_count = 0
        self.testing_size = 0
        self.outlier_lines_count = 0
        if not isinstance(targets, list):
            targets = [targets]  # type: ignore
        if not len(targets):
            raise RuntimeError("Empty testing targets")

        self.targets = targets

        for filename, filename_rel in files_iterator(targets, self.keep_file):
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
                model_name = self.filename_to_modelname(filename_rel)
                if model_name not in self.models:
                    self.log.debug(
                        "Skipping unknown file %s (%s)" % (filename, model_name)
                    )
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
            test_data: List[str] = []
            test_data_set: Set[str] = set()
            model = self.get(model_name)

            fobj = None
            try:
                fobj = open_file(filename)
                idx = 0
                for bline in fobj:
                    line = bline.decode("ascii", errors="ignore")
                    # Special case to not test ourself
                    if self._is_log_classify_invocation(model_name, line):
                        break
                    data.append(line)
                    line = self.process_line(line)
                    if line and line not in test_data_set:
                        test_data.append(line)
                        test_data_set.add(line)
                        test_data_pos.append(idx)
                    idx += 1
                del test_data_set
                try:
                    if isinstance(filename, str):
                        self.testing_size += os.stat(filename).st_size
                except TypeError:
                    pass
                self.testing_lines_count += idx
            except UnicodeDecodeError:
                self.log.info("%s: not a valid utf-8 file", filename)
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
                # Distances are a list of float list.
                # The HashingNeighbors vectorizer uses n_neighbors=1 to only
                # return the closest distance to a known baseline vector.
                distances = model.test(test_data)
            except (
                sklearn.utils.validation.NotFittedError,
                sklearn.exceptions.NotFittedError,
            ):
                self.log.warning("%s: skipping unfitted model" % filename)
                continue

            def get_line_info(line_pos):
                line = data[line_pos]
                try:
                    # Only keep the first distance
                    distance = distances[test_data_pos.index(line_pos)][0]
                except ValueError:
                    # Line wasn't in test data
                    distance = 0.0
                return (distance, line)

            # Store (line_pos, distance, line) in outliers
            outliers: List[Tuple[int, float, str]] = []
            last_outlier = 0
            remaining_after_context = 0
            for line_pos in range(len(data)):
                distance, line = get_line_info(line_pos)
                if distance >= self.threshold:
                    if line_pos - last_outlier >= self.merge_distance:
                        # When last outlier is too far,
                        # set last_outlier to before_context
                        last_outlier = max(line_pos - 1 - self.before_context, -1)
                    # Add previous context
                    for prev_pos in range(last_outlier + 1, line_pos):
                        outliers.append((prev_pos,) + get_line_info(prev_pos))
                    last_outlier = line_pos

                    outliers.append((line_pos, distance, line))
                    self.outlier_lines_count += 1
                    remaining_after_context = self.after_context
                elif remaining_after_context > 0:
                    outliers.append((line_pos, distance, line))
                    remaining_after_context -= 1
                    last_outlier = line_pos

            # Yield result
            yield (
                filename_rel,
                filename_orig,
                model,
                outliers,
                time.monotonic() - test_start_time,
            )

        self.test_time = time.monotonic() - start_time
        self.log.info(
            "Testing took %s"
            % format_speed(self.testing_lines_count, self.testing_size, self.test_time)
        )
        if not self.testing_lines_count:
            raise RuntimeError("No test lines found")

    def process(
        self,
        path: Sequence[LogObject],
        path_source: Optional[str] = None,
        threshold: float = 0.2,
        merge_distance: int = 5,
        before_context: int = 3,
        after_context: int = 1,
        console_output: bool = False,
        command: List[str] = sys.argv,
    ) -> Result:
        """Process target and create a report"""
        start_time = time.monotonic()
        self.threshold = threshold
        self.merge_distance = merge_distance
        self.before_context = before_context
        self.after_context = after_context
        # Initial Result type is ignored because it is currently incomplete.
        output: Result = {  # type: ignore
            "files": {},
            "unknown_files": [],
            "models": {},
            "anomalies_count": 0,
            "baselines": self.baselines,
        }
        for file_result in self.test(path):
            filename, filename_orig, model, outliers, test_time = file_result
            if model is None:
                # Do not bother with failed only logfile
                if "failed_deployment_list.log.txt" not in filename:
                    output["unknown_files"].append((filename, filename_orig))
                continue
            if path_source is not None:
                filename_orig = os.path.join(path_source, filename_orig)
            output["models"].setdefault(
                model.name,
                {
                    "source_files": list(map(str, model.sources)),
                    "train_time": model.train_time,
                    "info": model.info,
                    "uuid": model.uuid,
                },
            )
            file_info = output["files"].setdefault(
                filename,
                {
                    "file_url": filename_orig,
                    "test_time": test_time,
                    "model": model.name,
                    "scores": [],
                    "lines": [],
                },
            )
            last_pos = None
            self.log.debug(
                "%s: compared with %s"
                % (filename, " ".join(list(map(str, model.sources))))
            )

            if outliers:
                for pos, distance, outlier in outliers:
                    # Expand one-liner outputs (e.g. ansible)
                    for line in outlier[:-1].split(r"\n"):
                        line = line.replace(r"\t", "\t")
                        file_info["scores"].append((pos, distance))
                        file_info["lines"].append(line)
                        if console_output:
                            if last_pos and last_pos != pos and pos - last_pos != 1:
                                print("--")
                            print(
                                "%1.3f | %s:%04d:\t%s"
                                % (distance, filename, pos + 1, line)
                            )
                            last_pos = pos

            # Compute mean distances of outliers
            mean_distance = 0
            if file_info["scores"]:
                # [:, 1] returns an 1d array with the distances only
                mean_distance = np.mean(np.array(file_info["scores"])[:, 1])
                # TODO: do not cound sequential lines, only blocks
                output["anomalies_count"] += len(file_info["scores"])
            file_info["mean_distance"] = mean_distance

        output["targets"] = self.targets
        output["training_lines_count"] = self.training_lines_count
        output["testing_lines_count"] = self.testing_lines_count
        output["outlier_lines_count"] = self.outlier_lines_count
        output["reduction"] = (
            100 - (output["outlier_lines_count"] / output["testing_lines_count"]) * 100
        )
        test_command = " ".join(command)
        if test_command != self.train_command:
            output["train_command"] = self.train_command
        output["test_command"] = test_command
        output["total_time"] = time.monotonic() - start_time
        return output
