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

import unittest

from logreduce.process import Classifier
from logreduce.tokenizer import Tokenizer


class TokenizerTests(unittest.TestCase):
    def test_random_words(self):
        tokens = Tokenizer.process("Created interface: br-42")
        self.assertNotIn("br-42", tokens)
        tokens = Tokenizer.process("Instance 0xdeadbeef42 created")
        self.assertEquals("Instance created", tokens)

    def test_filename2modelname(self):
        for fname, modelname in (
                ("builds/2/log", "log"),
                ("audit/audit.log", "audit/audit.log"),
                ("audit/audit.log.1", "audit/audit.log"),
                ("zuul/merger.log.2017-11-12", "zuul/merger.log"),
                ("conf.d/00-base.conf.txt.gz", "conf.d/-base.conf.txt"),
                ("jobs/test-sleep-217/config.xml", "test-sleep-/config.xml"),
        ):
            name = Classifier.filename2modelname(fname)
            self.assertEqual(name, modelname)
