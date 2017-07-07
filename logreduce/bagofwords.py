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
import time
import warnings

from sklearn.externals import joblib
from sklearn.feature_extraction.text import CountVectorizer
from sklearn.feature_extraction.text import TfidfTransformer
from sklearn.neighbors import LSHForest

from logreduce.utils import files_iterator
from logreduce.utils import open_file
from logreduce.utils import Tokenizer


# Parameters
RANDOM_STATE=int(os.environ.get("LR_RANDOM_STATE", 42))
USE_IDF=bool(int(os.environ.get("LR_USE_IDF", 1)))
N_ESTIMATORS=int(os.environ.get("LR_N_ESTIMATORS", 23))


class BagOfWords:
    log = logging.getLogger("BagOfWords")

    def __init__(self, threshold):
        self.bags = {}
        self.threshold = float(threshold)
        self.training_lines_count = 0

    def get(self, bagname):
        return self.bags.setdefault(bagname, (
            [], # source files
            CountVectorizer(analyzer='word', lowercase=False, tokenizer=None,
                            preprocessor=None, stop_words=None),
            TfidfTransformer(use_idf=USE_IDF),
            LSHForest(random_state=RANDOM_STATE, n_estimators=N_ESTIMATORS),
        ))

    def save(self, fileobj):
        """Save the model"""
        joblib.dump(self, fileobj, compress=True)
        print("%s: written" % fileobj)

    @staticmethod
    def load(fileobj):
        """Load a saved model"""
        return joblib.load(fileobj)

    def train(self, path):
        """Train the model"""
        start_time = time.time()

        # Group similar files for the same model
        to_train = {}
        for filename, _, fileobj in files_iterator(path):
            bag_name = Tokenizer.filename2modelname(filename)
            to_train.setdefault(bag_name, []).append(filename)
            fileobj.close()

        # Train each model
        for bag_name, filenames in to_train.items():
            # Tokenize and store all lines in train_data
            train_data = []
            for filename in filenames:
                self.log.debug("%s: Loading %s" % (bag_name, filename))
                try:
                    data = open_file(filename).readlines()
                except:
                    self.log.exception("%s: couldn't read" % filename)
                    continue
                idx = 0
                while idx < len(data):
                    line = Tokenizer.process(data[idx][:-1])
                    if ' ' in line:
                        # We need at least two words
                        train_data.append(line)
                    idx += 1
                self.training_lines_count += idx
            if not train_data:
                continue

            try:
                # Transform and fit the model data
                files, count_vect, tfidf_transformer, lshf = self.get(bag_name)
                for filename in filenames:
                    files.append(filename)

                with warnings.catch_warnings():
                    warnings.simplefilter("ignore")
                    train_count = count_vect.fit_transform(train_data)
                    train_tfidf = tfidf_transformer.fit_transform(train_count)
                    lshf.fit(train_tfidf)
            except ValueError:
                self.log.warning("%s: couldn't train with %s" % (bag_name,
                                                                 train_data))
                del self.bags[bag_name]
            except:
                self.log.exception("%s: couldn't train with %s" % (bag_name,
                                                                   train_data))
                del self.bags[bag_name]
        end_time = time.time()
        train_speed = self.training_lines_count / (end_time - start_time)
        self.log.debug("Training took %.03f seconds to ingest %d lines (%.03f kl/s)" % (
            end_time - start_time, self.training_lines_count, train_speed / 1000))
        if not self.training_lines_count:
            raise RuntimeError("No train lines found")
        return self.training_lines_count


#    @profile
    def test(self, path, merge_distance, before_context, after_context):
        """Return outliers"""
        start_time = time.time()
        self.testing_lines_count = 0
        self.outlier_lines_count = 0

        for filename, filename_orig, fileobj in files_iterator(path):
            if len(self.bags) > 1:
                # Get model name based on filename
                bag_name = Tokenizer.filename2modelname(filename)
                if bag_name not in self.bags:
                    self.log.debug("Skipping unknown file %s (%s)" % (filename, bag_name))
                    continue
            else:
                # Only one file was trained, use it's model
                bag_name = list(self.bags.keys())[0]
            self.log.debug("%s: Testing %s" % (bag_name, filename))

            try:
                data = fileobj.readlines()
            except:
                self.log.exception("%s: couldn't read" % fileobj.name)
                continue

            # Store file line number in test_data_pos
            test_data_pos = []
            # Tokenize and store all lines in test_data
            test_data = []
            idx = 0
            while idx < len(data):
                line = Tokenizer.process(data[idx][:-1])
                if ' ' in line:
                    # We need at least two words
                    test_data.append(line)
                    test_data_pos.append(idx)
                idx += 1
            self.testing_lines_count += idx
            if not test_data:
                self.log.warning("%s: not valid lines" % filename)
                continue

            # Transform and compute distance from the model
            files, count_vect, tfidf_transformer, lshf = self.bags[bag_name]
            with warnings.catch_warnings():
                warnings.simplefilter("ignore")
                test_count = count_vect.transform(test_data)
                test_tfidf = tfidf_transformer.transform(test_count)
                distances, _ = lshf.kneighbors(test_tfidf, n_neighbors=1)

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
                    if line_pos - last_outlier >= merge_distance:
                        # When last outlier is too far, set last_outlier to before_context
                        last_outlier = max(line_pos - 1 - before_context, -1)
                    # Add previous context
                    for prev_pos in range(last_outlier + 1, line_pos):
                        outliers.append(get_line_info(prev_pos))
                    last_outlier = line_pos

                    outliers.append((line_pos, distance, line))
                    self.outlier_lines_count += 1
                    remaining_after_context = after_context
                elif remaining_after_context > 0:
                    outliers.append((line_pos, distance, line))
                    remaining_after_context -= 1
                    last_outlier = line_pos
                line_pos += 1

            # Yield result
            yield (filename_orig, files, outliers)

        end_time = time.time()
        test_speed = self.testing_lines_count / (end_time - start_time)
        self.log.debug("Testing took %.03f seconds to test %d lines (%.03f kl/s)" % (
            end_time - start_time, self.testing_lines_count, test_speed / 1000))
        if not self.testing_lines_count:
            raise RuntimeError("No test lines found")
