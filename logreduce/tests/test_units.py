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
    def check_expected(self, tests):
        for raw_line, tokens_out in tests.items():
            self.assertEqual(
                tokens_out, Tokenizer.process(raw_line))

    def test_random_words(self):
        tokens = Tokenizer.process("Created interface: br-42")
        self.assertNotIn("br-42", tokens)
        tokens = Tokenizer.process("Instance 0xdeadbeef42 created")
        self.assertEqual("Instance created", tokens)

    def test_hash_tokenizing(self):
        self.check_expected({
            'Accepted publickey: RSA '
            'SHA256:UkrwIX8QHA4B2Bny0XHyqgSXM7wFMQTEDtT+PpY9Ep4':
            'Accepted publickey RNGH',
            # This used to match 'jan' -> DATE
            'SHA256:FePTgARR5A3kxb2GJa0QAWjanaI2q+TvneBxzHNqbTA zuul@ze03':
            'RNGH zuul'
        })

    def test_ipv6_tokenizing(self):
        self.check_expected({
            'mysql+pymysql://root:secretdatabase@[::1]/cinder?"':
            'mysql pymysql //root secretdatabase RNGI /cinder',
            'bind-address=fd00:fd00:fd00:2000::1f':
            'bind address RNGI',
            'listen_port fe80::f816:3eff:fe47:5142':
            'listen_port RNGI',
            'listen_port FE80::F816:3eff:fe47:5142':
            'listen_port RNGI',
            'listen_port ::8888':
            'listen_port RNGI'
        })

    def test_date_non_tokenizing(self):
        """Tests that should not match the DATE verb"""
        self.check_expected({
            'keys randomart image':
            'keys randomart image',
            'Start zuul_console daemon':
            'Start zuul_console daemon',
        })

    def test_uuid_words(self):
        self.check_expected({
            '| 0473427f-f505-4b50-bc70-72fb6d74568a | vmname | SHUTOFF | -   '
            '       | Shutdown    | fixed=192.168.123.3 |':
            'RNGU vmname SHUTOFF Shutdown fixed RNGI',
            '"UndercloudServiceChain-2kbhkd45kcs3-ServiceChain-54rklv3rnxhe" ':
            'UndercloudServiceChain HEATID ServiceChain HEATID',
            'GET /1/339/AUTH_4f271e48b7dd480f916056948c76dd7f/'
            'zaqar_subscription%3Atripleo%3A832822a81f7a4917ac5321b72fcdbbf5'
            '/f0d6a4ec-9f94-4c12-bb6b-d1943856410e':
            '///AUTHRNGN/zaqar_subscription tripleo RNGN/RNGU'
        })

    def test_non_uuid_words(self):
        self.check_expected({
            'dnsmasq-dhcp[31216]: DHCPRELEASE':
            'dnsmasq dhcp DHCPRELEASE',
        })

    def test_digits_tokenizing(self):
        self.check_expected({
            'Started Session 2677 of user root':
            'Started Session user root',
            'Instance 0xdeadbeef42 created':
            'Instance created',
            'systemd[4552]: Startup finished in 28ms.':
            'systemd Startup finished',
            '764928K 33%  469M 3.05s':
            ''
        })

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
