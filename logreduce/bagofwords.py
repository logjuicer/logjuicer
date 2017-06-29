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
import time

from sklearn.externals import joblib
from sklearn.feature_extraction.text import CountVectorizer
from sklearn.feature_extraction.text import TfidfTransformer
from sklearn.neighbors import LSHForest

from logreduce.utils import files_iterator
from logreduce.utils import Tokenizer


class BagOfWords:
    log = logging.getLogger("BagOfWords")

    def __init__(self, max_distance, debug_token, use_idf=True):
        self.bags = {}
        self.max_distance = float(max_distance)
        self.use_idf = use_idf
        self.debug_token = bool(debug_token)
        self.training_lines_count = 0

    def get(self, bagname):
        return self.bags.setdefault(bagname, (
            [], # source files
            CountVectorizer(analyzer='word', lowercase=False),
            TfidfTransformer(use_idf=self.use_idf),
            LSHForest(random_state=42),  # , n_neighbors=1, n_candidates=1,
            # radius=self.max_distance)
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
        for filename, fileobj in files_iterator(path):
            bag_name = Tokenizer.filename2modelname(filename)
            to_train.setdefault(bag_name, []).append(fileobj)

        # Train each model
        for bag_name, fileobjs in to_train.items():
            # Tokenize and store all lines in train_data
            train_data = []
            for fileobj in fileobjs:
                self.log.debug("%s: Loading %s" % (bag_name, fileobj.name))
                try:
                    data = fileobj.readlines()
                except:
                    self.log.exception("%s: couldn't read" % fileobj.name)
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
                for fileobj in fileobjs:
                    files.append(fileobj.name)
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
        self.log.debug("Training took %.03f seconds to ingest %d lines" % (
            time.time() - start_time, self.training_lines_count))
        return self.training_lines_count


#    @profile
    def test(self, path, merge_distance, before_context, after_context):
        """Return outliers"""
        start_time = time.time()
        self.testing_lines_count = 0
        self.outlier_lines_count = 0

        for filename, fileobj in files_iterator(path):
            # Get model name based on filename
            bag_name = Tokenizer.filename2modelname(filename)
            if bag_name not in self.bags:
                self.log.debug("Skipping unknown file %s" % filename)
                continue
            self.log.debug("%s: Testing %s" % (bag_name, filename))

            try:
                data = fileobj.readlines()
            except:
                self.log.exception("%s: couldn't read" % fileobj.name)
                continue
            idx = 0

            # Store file line number in test_data_pos
            test_data_pos = []
            # Tokenize and store all lines in test_data
            test_data = []
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
            test_count = count_vect.transform(test_data)
            test_tfidf = tfidf_transformer.transform(test_count)
            distances, _ = lshf.kneighbors(test_tfidf, n_neighbors=1)

            # Store (line_pos, distance, line) in outliers
            outliers = []
            last_outlier = 0
            remaining_after_context = 0
            idx = 0
            while idx < len(test_data):
                line_pos = test_data_pos[idx]
                if distances[idx] >= self.max_distance:

                    # Add context
                    if line_pos - last_outlier >= merge_distance:
                        # Add previous line when too far apart
                        last_outlier = max(line_pos - 1 - before_context, -1)
                    for prev_pos in range(last_outlier + 1, line_pos):
                        outliers.append((prev_pos, 0, data[prev_pos]))
                        self.outlier_lines_count += 1
                    last_outlier = line_pos

                    outliers.append((line_pos, distances[idx], data[line_pos]))
                    remaining_after_context = after_context
                elif remaining_after_context > 0:
                    outliers.append((line_pos, distances[idx], data[line_pos]))
                    remaining_after_context -= 1
                idx += 1

            # Yield result
            yield (filename, files, outliers)
        self.log.debug("Testing took %.03f seconds to test %d lines" % (
            time.time() - start_time, self.testing_lines_count))
