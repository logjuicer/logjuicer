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
from sklearn.feature_extraction.text import TfidfVectorizer
from sklearn.neighbors import LSHForest
from sklearn.neighbors import NearestNeighbors
from sklearn import svm

from logreduce.tokenizer import Tokenizer
from logreduce.tokenizer import remove_ansible_std_lines_lists
from logreduce.utils import files_iterator
from logreduce.utils import open_file


# Query chunk size, it seems to improve memory footprint of kneighbors call
CHUNK_SIZE = int(os.environ.get("LR_CHUNK_SIZE", 512))
# Disable multiprocessing by default
os.environ["JOBLIB_MULTIPROCESSING"] = os.environ.get("LR_MULTIPROCESSING",
                                                      "0")


class Model:
    """Base class for model"""
    def __init__(self):
        self.sources = []

    def process_line(self, line):
        """Process log lines"""
        return Tokenizer.process(line)

    def train(self, train_data):
        """Fit the model with train_datas"""
        pass

    def test(self, test_data):
        """Detect outliers, return list of distances"""
        pass


class Noop(Model):
    """Benchmark model"""
    def test(self, test_data):
        return [0.5] * len(test_data)


class LSHF(Model):
    """Forest model, faster for on large index (>20000 samples)"""
    def __init__(self):
        super(LSHF, self).__init__()
        self.vectorizer = TfidfVectorizer(
            analyzer='word', lowercase=False, tokenizer=None,
            preprocessor=None, stop_words=None)

        self.lshf = LSHForest(
            random_state=int(os.environ.get("LR_RANDOM_STATE", 42)),
            n_estimators=int(os.environ.get("LR_N_ESTIMATORS", 23)))

    def train(self, train_data):
        with warnings.catch_warnings():
            warnings.simplefilter("ignore")
            dat = self.vectorizer.fit_transform(train_data)
            self.lshf.fit(dat)
        return dat

    def test(self, test_data):
        all_distances = []
        with warnings.catch_warnings():
            for chunk_pos in range(0, len(test_data), CHUNK_SIZE):
                chunk = test_data[chunk_pos:min(len(test_data),
                                                chunk_pos + CHUNK_SIZE)]
                dat = self.vectorizer.transform(chunk)
                distances, _ = self.lshf.kneighbors(dat, n_neighbors=1)
                all_distances.extend(distances)
        return all_distances


class SimpleNeighbors(Model):
    """Simple NN model"""
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


class SimpleNeighborsBin(Model):
    """NN model using power2 bin"""
    BINS = (4, 8, 16, 32, 64)

    def __init__(self):
        super(SimpleNeighborsBin, self).__init__()
        self.bins = [None] * len(SimpleNeighborsBin.BINS)

    def train(self, train_data):
        bins = [None] * len(SimpleNeighborsBin.BINS)
        for line in train_data:
            binsize = len(line.split())
            for idx in range(len(SimpleNeighborsBin.BINS)):
                if binsize <= SimpleNeighborsBin.BINS[idx]:
                    break
            if self.bins[idx] is None:
                self.bins[idx] = (
                    TfidfVectorizer(
                        analyzer='word', lowercase=False, tokenizer=None,
                        preprocessor=None, stop_words=None),
                    NearestNeighbors(
                        algorithm='brute',
                        metric='cosine'),
                )
                bins[idx] = []
            bins[idx].append(line)

        for idx in range(len(SimpleNeighborsBin.BINS)):
            if bins[idx] is None:
                continue
            dat = self.bins[idx][0].fit_transform(bins[idx])
            self.bins[idx][1].fit(dat)
        return dat

    def test(self, test_data):
        all_distances = []
        for line in test_data:
            binsize = len(line.split())
            for idx in range(len(SimpleNeighborsBin.BINS)):
                if binsize <= SimpleNeighborsBin.BINS[idx]:
                    break
            if self.bins[idx] is None:
                all_distances.append(0.9)
                continue
            dat = self.bins[idx][0].transform([line])
            distances, _ = self.bins[idx][1].kneighbors(dat, n_neighbors=1)
            all_distances.append(distances[0])
        return all_distances


class Hash(Model):
    """Experimental model"""
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
        'bag-of-words_bin-nn': SimpleNeighborsBin,
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

    @staticmethod
    def filename2modelname(filename):
        """Create a modelname based on filename"""
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

    def train(self, path):
        """Train the model"""
        start_time = time.time()

        # Group similar files for the same model
        to_train = {}
        for filename, filename_rel in files_iterator(path):
            bag_name = OutliersDetector.filename2modelname(filename_rel)
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
                fobj = None
                try:
                    fobj = open_file(filename)
                    idx = 0
                    while True:
                        line = fobj.readline()
                        if line == b'':
                            break
                        line = line.decode('ascii', errors='ignore')
                        # Remove ansible std_lines list now
                        line = remove_ansible_std_lines_lists(line)
                        for sub_line in line.split(r'\r'):
                            sub_line = model.process_line(sub_line)
                            if sub_line:
                                train_data.append(sub_line)
                        idx += 1
                    bag_size += os.stat(filename).st_size
                    bag_count += idx
                except:
                    self.log.exception("%s: couldn't read" % filename)
                    continue
                finally:
                    if fobj:
                        fobj.close()
                url_prefix = "/tmp/logreduce-getthelogs/"
                if filename.startswith(url_prefix):
                    forig = "http://%s" % (filename[len(url_prefix):])
                else:
                    forig = filename
                model.sources.append(forig)

            if not train_data:
                self.log.info("%s: no training data found" % bag_name)
                continue

            self.training_lines_count += bag_count
            self.training_size += bag_size
            try:
                # Transform and fit the model data
                model = self.get(bag_name)
                train_result = model.train(train_data)

                # Collect statistics
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
                self.log.exception("%s: couldn't train with %s" % (bag_name,
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

        for filename, filename_rel in files_iterator(path):
            url_prefix = "/tmp/logreduce-getthelogs/"
            if filename.startswith(url_prefix):
                filename_orig = "http://%s" % (filename[len(url_prefix):])
            else:
                filename_orig = filename

            if len(self.bags) > 1:
                # Get model name based on filename
                bag_name = OutliersDetector.filename2modelname(filename_rel)
                if bag_name not in self.bags:
                    self.log.debug("Skipping unknown file %s (%s)" % (
                        filename, bag_name))
                    yield (filename_rel, filename_orig, None, None)
                    continue
            else:
                # Only one file was trained, use its model
                bag_name = list(self.bags.keys())[0]
            self.log.debug("%s: Testing %s" % (bag_name, filename))

            # Store file line number in test_data_pos
            data = []
            test_data_pos = []
            # Tokenize and store all lines in test_data
            test_data = []
            test_data_set = set()
            model = self.get(bag_name)

            fobj = None
            try:
                fobj = open_file(filename)
                idx = 0
                while True:
                    line = fobj.readline()
                    if line == b'':
                        break
                    line = line.decode('ascii', errors='ignore')
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
            except:
                self.log.exception("%s: couldn't read" % filename)
                continue
            finally:
                if fobj:
                    fobj.close()

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
            yield (filename_rel, filename_orig, model.sources, outliers)

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
