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
import time
import warnings

import sklearn.utils.validation

from sklearn.externals import joblib
from sklearn.feature_extraction.text import CountVectorizer
from sklearn.feature_extraction.text import TfidfTransformer
from sklearn.feature_extraction.text import TfidfVectorizer
from sklearn.neighbors import LSHForest
from sklearn.neighbors import NearestNeighbors
from sklearn import svm

from logreduce.utils import files_iterator
from logreduce.utils import open_file
from logreduce.utils import Tokenizer


# Parameters
RANDOM_STATE = int(os.environ.get("LR_RANDOM_STATE", 42))
USE_IDF = bool(int(os.environ.get("LR_USE_IDF", 1)))
N_ESTIMATORS = int(os.environ.get("LR_N_ESTIMATORS", 23))
CHUNK_SIZE = int(os.environ.get("LR_CHUNK_SIZE", 512))
# Disable multiprocessing by default
os.environ["JOBLIB_MULTIPROCESSING"] = os.environ.get("LR_MULTIPROCESSING",
                                                      "0")


class Model:
    def __init__(self):
        self.sources = []

    def process_line(self, line):
        """Process log lines, return reduced version only if revelant"""
        line = Tokenizer.process(line)
        if ' ' in line:
            # We need at least two words
            return line

    def train(self, train_data):
        """Fit the model with train_datas"""
        pass

    def test(self, test_data):
        """Detect outliers, return list of distances"""
        pass


class Noop(Model):
    def test(self, test_data):
        return [0.5] * len(test_data)


class LSHF(Model):
    def __init__(self):
        super(LSHF, self).__init__()
        self.count_vect = CountVectorizer(
            analyzer='word', lowercase=False, tokenizer=None,
            preprocessor=None, stop_words=None)
        self.tfidf_transformer = TfidfTransformer(use_idf=USE_IDF)
        self.lshf = LSHForest(random_state=RANDOM_STATE,
                              n_estimators=N_ESTIMATORS)

    def train(self, train_data):
        with warnings.catch_warnings():
            warnings.simplefilter("ignore")
            train_count = self.count_vect.fit_transform(train_data)
            train_tfidf = self.tfidf_transformer.fit_transform(train_count)
            self.lshf.fit(train_tfidf)
        return train_tfidf

    def test(self, test_data):
        all_distances = []
        with warnings.catch_warnings():
            for chunk_pos in range(0, len(test_data), CHUNK_SIZE):
                chunk = test_data[chunk_pos:min(len(test_data),
                                                chunk_pos + CHUNK_SIZE)]
                test_count = self.count_vect.transform(chunk)
                test_tfidf = self.tfidf_transformer.transform(test_count)
                distances, _ = self.lshf.kneighbors(test_tfidf, n_neighbors=1)
                all_distances.extend(distances)
        return all_distances


class SimpleNeighbors(Model):
    log = logging.getLogger("SimpleNeighbors")

    def __init__(self):
        super(SimpleNeighbors, self).__init__()
        self.vectorizer = TfidfVectorizer(
            analyzer='word', lowercase=False, tokenizer=None,
            preprocessor=None, stop_words=None)
        self.nn = NearestNeighbors(
            algorithm='brute',
            metric='cosine')

    def train(self, train_data):
        dat = self.vectorizer.fit_transform(train_data)
        self.nn.fit(dat)
        return dat

    def test(self, test_data):
        all_distances = []
        with warnings.catch_warnings():
            for chunk_pos in range(0, len(test_data), CHUNK_SIZE):
                chunk = test_data[chunk_pos:min(len(test_data),
                                                chunk_pos + CHUNK_SIZE)]
                dat = self.vectorizer.transform(chunk)
                distances, _ = self.nn.kneighbors(dat, n_neighbors=1)
                all_distances.extend(distances)
        return all_distances


class Hash(Model):
    log = logging.getLogger("Hash")
    remove_re = re.compile(r'[0-9]')
    split_re = re.compile(r'[/\-\.]')

    def __init__(self):
        super(Hash, self).__init__()
        self.hashes = []
        self.max_size = 0
        self.nn = NearestNeighbors(
            algorithm='brute',
            metric='cosine')
        self.svm = svm.OneClassSVM(nu=0.1, kernel="rbf", gamma=0.1)

    def hash(self, line):
        hs = [0] * 255
        split = Hash.remove_re.subn("", line)[0][:-1]
        split = Hash.split_re.subn(" ", split)[0]
        split = split.split()
        for i in split:
            h = 0
            for c in i:
                h ^= (ord(c) & 0xff)
            hs[h] = 1
        return hs

    def process_line(self, line):
        if not line:
            return None
        hs = self.hash(line)
        if len(hs) > self.max_size:
            self.max_size = len(hs)
        return hs

    def train(self, train_data):
        # Pad hashes
        for hs in train_data:
            d = self.max_size - len(hs)
            if d:
                hs.extend([0] * d)
        self.nn.fit(train_data)

    def test(self, test_data):
        # Pad hashes
        for hs in test_data:
            d = self.max_size - len(hs)
            if d:
                hs.extend([0] * d)
        distances, _ = self.nn.kneighbors(test_data, n_neighbors=1)
        return distances


class OutliersDetector:
    log = logging.getLogger("OutlierDetector")
    models = {
        'bag-of-words_lshf': LSHF,
        'bag-of-words_nn': SimpleNeighbors,
        'bag-of-hash_nn': Hash,
        'noop': Noop,
    }

    def __init__(self, model='lshf'):
        self.bags = {}
        self.model = model
        self.training_lines_count = 0
        self.training_size = 0

    def get(self, bagname):
        return self.bags.setdefault(bagname,
                                    OutliersDetector.models[self.model]())

    def save(self, fileobj):
        """Save the model"""
        joblib.dump(self, fileobj, compress=True)
        self.log.info("%s: written" % fileobj)

    @staticmethod
    def load(fileobj):
        """Load a saved model"""
        return joblib.load(fileobj)

    def train(self, path):
        """Train the model"""
        start_time = time.time()

        # Group similar files for the same model
        to_train = {}
        for filename in files_iterator(path):
            bag_name = Tokenizer.filename2modelname(filename[len(path):])
            to_train.setdefault(bag_name, []).append(filename)

        # Train each model
        for bag_name, filenames in to_train.items():
            # Tokenize and store all lines in train_data
            train_data = []
            bag_start_time = time.time()
            bag_size = 0
            bag_count = 0
            model = self.get(bag_name)
            for filename in filenames:
                self.log.debug("%s: Loading %s" % (bag_name, filename))
                try:
                    data = open_file(filename).readlines()
                    bag_size += os.stat(filename).st_size
                except:
                    self.log.exception("%s: couldn't read" % filename)
                    continue
                idx = 0
                while idx < len(data):
                    line = model.process_line(data[idx][:-1])
                    if line:
                        train_data.append(line)
                    idx += 1
                bag_count += idx

            if not train_data:
                self.log.info("%s: no training data found" % bag_name)
                continue

            self.training_lines_count += bag_count
            self.training_size += bag_size
            try:
                # Transform and fit the model data
                model = self.get(bag_name)
                for filename in filenames:
                    model.sources.append(filename)
                train_result = model.train(train_data)

                bag_train_time = time.time() - bag_start_time

                bag_size_speed = (bag_size / (1024 * 1024)) / bag_train_time
                bag_count_speed = (bag_count / 1000) / bag_train_time
                try:
                    n_samples, n_features = train_result.shape
                except:
                    n_samples, n_features = 0, 0
                self.log.debug("%s: took %.03fs at %.03fMb/s (%.03fkl/s): "
                               "%d samples, %d features" % (
                                   bag_name, bag_train_time, bag_size_speed,
                                   bag_count_speed, n_samples, n_features))
            except ValueError:
                self.log.warning("%s: couldn't train with %s" % (bag_name,
                                                                 train_data))
                del self.bags[bag_name]
            except:
                self.log.exception("%s: couldn't train with %s" % (bag_name,
                                                                   train_data))
                del self.bags[bag_name]
        train_time = time.time() - start_time
        train_size_speed = (self.training_size / (1024 * 1024)) / train_time
        train_count_speed = (self.training_lines_count / 1000) / train_time
        self.log.debug("Training took %.03f seconds to ingest %.03f MB "
                       "(%.03f MB/s) or %d lines (%.03f kl/s)" % (
                           train_time,
                           self.training_size / (1024 * 1024),
                           train_size_speed,
                           self.training_lines_count,
                           train_count_speed))
        if not self.training_lines_count:
            raise RuntimeError("No train lines found")
        return self.training_lines_count


#    @profile
    def test(self, path, threshold, merge_distance,
             before_context, after_context):
        """Return outliers"""
        start_time = time.time()
        self.testing_lines_count = 0
        self.testing_size = 0
        self.outlier_lines_count = 0

        for filename in files_iterator(path):
            if len(self.bags) > 1:
                # Get model name based on filename
                bag_name = Tokenizer.filename2modelname(filename[len(path):])
                if bag_name not in self.bags:
                    self.log.debug("Skipping unknown file %s (%s)" % (
                        filename, bag_name))
                    continue
            else:
                # Only one file was trained, use it's model
                bag_name = list(self.bags.keys())[0]
            self.log.debug("%s: Testing %s" % (bag_name, filename))

            try:
                data = open_file(filename).readlines()
                self.testing_size += os.stat(filename).st_size
            except:
                self.log.exception("%s: couldn't read" % filename)
                continue

            # Store file line number in test_data_pos
            test_data_pos = []
            # Tokenize and store all lines in test_data
            test_data = []
            model = self.get(bag_name)
            idx = 0
            while idx < len(data):
                line = model.process_line(data[idx][:-1])
                if line:
                    # We need at least two words
                    test_data.append(line)
                    test_data_pos.append(idx)
                idx += 1
            self.testing_lines_count += idx
            if not test_data:
                self.log.warning("%s: not valid lines" % filename)
                continue

            # Transform and compute distance from the model
            model = self.bags[bag_name]
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
                if distance >= threshold:
                    if line_pos - last_outlier >= merge_distance:
                        # When last outlier is too far,
                        # set last_outlier to before_context
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
            yield (filename, model.sources, outliers)

        test_time = time.time() - start_time
        test_size_speed = (self.testing_size / (1024 * 1024)) / test_time
        test_count_speed = (self.testing_lines_count / 1000) / test_time
        self.log.debug("Testing took %.03f seconds to test %.03f MB "
                       "(%.03f MB/s) or %d lines (%.03f kl/s)" % (
                           test_time,
                           self.testing_size / (1024 * 1024),
                           test_size_speed,
                           self.testing_lines_count,
                           test_count_speed))
        if not self.testing_lines_count:
            raise RuntimeError("No test lines found")
