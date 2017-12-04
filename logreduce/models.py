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

import os
import re
import warnings

from sklearn.feature_extraction.text import TfidfVectorizer
from sklearn.neighbors import LSHForest
from sklearn.neighbors import NearestNeighbors
from sklearn import svm

from logreduce.tokenizer import Tokenizer

# Query chunk size, it seems to improve memory footprint of kneighbors call
CHUNK_SIZE = int(os.environ.get("LR_CHUNK_SIZE", 512))
# Disable multiprocessing by default
os.environ["JOBLIB_MULTIPROCESSING"] = os.environ.get(
    "LR_MULTIPROCESSING", "0")


class Model:
    """Base class for model"""
    def __init__(self, name):
        self.name = name
        self.sources = []
        self.train_time = 0
        self.info = ""

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
    def __init__(self, name=""):
        super().__init__(name)
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
        self.info = "%d samples, %d features" % dat.shape
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
    def __init__(self, name=""):
        super().__init__(name)
        self.vectorizer = TfidfVectorizer(
            analyzer='word', lowercase=False, tokenizer=None,
            preprocessor=None, stop_words=None)
        self.nn = NearestNeighbors(
            algorithm='brute',
            metric='cosine')

    def train(self, train_data):
        dat = self.vectorizer.fit_transform(train_data)
        self.nn.fit(dat)
        self.info = "%d samples, %d features" % dat.shape
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

    def __init__(self, name=""):
        super().__init__(name)
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

    def __init__(self, name=""):
        super().__init__(name)
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


models = {
    'bag-of-words_lshf': LSHF,
    'bag-of-words_nn': SimpleNeighbors,
    'bag-of-words_bin-nn': SimpleNeighborsBin,
    'bag-of-hash_nn': Hash,
    'noop': Noop,
}
