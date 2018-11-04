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
import warnings

from sklearn.feature_extraction.text import TfidfVectorizer
from sklearn.neighbors import NearestNeighbors
from sklearn.feature_extraction.text import HashingVectorizer
# from sklearn import svm

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


class SimpleNeighbors(Model):
    """Simple NN model"""
    def __init__(self, name=""):
        super().__init__(name)
        self.vectorizer = TfidfVectorizer(
            analyzer=str.split, lowercase=False, tokenizer=None,
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


class HashingNeighbors(Model):
    """HashingVectorized NN model"""
    # True random words
    def __init__(self, name=""):
        super().__init__(name)
        self.vectorizer = HashingVectorizer(
            binary=True, n_features=2**18,
            analyzer=str.split, lowercase=False, tokenizer=None,
            preprocessor=None, stop_words=None)
        # HashingVectorizer produces sparse vectors, and
        # sorted(sklearn.neighbors.VALID_METRICS_SPARSE['algorithm']) is
        # empty for anything != brute
        self.nn = NearestNeighbors(
            algorithm='brute', metric='cosine',
            n_jobs=1, n_neighbors=1)

    def train(self, train_data):
        dat = self.vectorizer.transform(train_data)
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
                distances, _ = self.nn.kneighbors(dat)
                all_distances.extend(distances)
        return all_distances


class HashingApproximateNeighbors(Model):
    """ Approximate Nearest Neighbor Search.
    This implementation is rather slow, logreduce-tests benchmark goes from
    12sec to 60sec.
    The code may be optimized to not record training data since we don't care
    what the actual neighbor is, and it should simply return distance as float
    and not str.

    TODO: benchmark with higher sample size.
    """
    def __init__(self, name=""):
        super().__init__(name)
        self.vectorizer = HashingVectorizer(
            binary=True,
            analyzer=str.split, lowercase=False, tokenizer=None,
            preprocessor=None, stop_words=None)

    def train(self, train_data):
        try:
            import pysparnn.cluster_index as ci
        except ImportError:
            raise RuntimeError("Install this dependency to use this model: "
                               "https://github.com/facebookresearch/pysparnn")
        train_data = list(train_data)
        dat = self.vectorizer.transform(train_data)
        self.nn = ci.MultiClusterIndex(dat, train_data)
        self.info = ''

    def test(self, test_data):
        all_distances = []
        for chunk_pos in range(0, len(test_data), CHUNK_SIZE):
            chunk = test_data[chunk_pos:min(len(test_data),
                                            chunk_pos + CHUNK_SIZE)]
            dat = self.vectorizer.transform(chunk)
            distances = self.nn.search(
                dat, k=1, k_clusters=2, return_distance=True)
            # Work around str format of distance...
            for distance in distances:
                if distance[0][0].startswith('-'):
                    all_distances.append([0.0])
                    continue
                all_distances.append([float(distance[0][0])])
        return all_distances


models = {
    'bag-of-words_nn': SimpleNeighbors,
    'hashing_nn': HashingNeighbors,
    'hashing_ann': HashingApproximateNeighbors,
    'noop': Noop,
}
