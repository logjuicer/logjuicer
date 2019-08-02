# Copyright 2018 Red Hat, Inc.
# Copyright 2018 SUSE Linux GmbH.
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

import re

# Tokenizer
DAYS = "sunday|monday|tuesday|wednesday|thursday|friday|saturday"
MONTHS = "january|february|march|april|may|june|july|august|september|" \
         "october|november|december"
SHORT_MONTHS = "jan|feb|mar|apr|may|jun|jul|aug|sep|oct|nov|dev"
SHORT_DAYS = "mon|tue|wed|thu|fri|sat|sun"

UUID_RE = r'[0-9a-f]{8}-?[0-9a-f]{4}-?[0-9a-f]{4}-?[0-9a-f]{4}-' \
          '?[0-9a-f]{12}'
IPV4_RE = r'(([01]?[0-9]?[0-9]|2[0-4][0-9]|2[5][0-5])\.){3}' \
          r'([01]?[0-9]?[0-9]|2[0-4][0-9]|2[5][0-5])'
IPV6_RE = r'([0-9A-Fa-f]{0,4}:){2,6}(\d{1,3}\.){0,3}[0-9A-Fa-f]{1,3}'
MAC_RE = r'([0-9a-fA-F]{2}[:-]){5}([0-9a-fA-F]{2})'


class Tokenizer:
    rawline_re = re.compile(
        # useless http GET
        r'"GET / HTTP/1.1"'
        r'|"OPTIONS * HTTP/1.0" 200'
        # ssh keys
        r'|AAAA[A-Z][0-9]'
        # hashed password
        r'|\$[0-9]\$'
        # Certificates
        r'|-----BEGIN'
        # git status
        r'|HEAD is now at|Change-Id: '
        # Download statement
        r'| ETA '
        # yum mirrors information
        r'|\* [a-zA-Z]+: [a-zA-Z0-9\.-]*$|Trying other mirror.'
        # ssh scan attempts
        r'|audit.*exe="/usr/sbin/sshd"|sshd.*[iI]nvalid user'
        r'|sshd.*Unable to connect using the available authentication methods'
        r'|unix_chkpwd.*: password check failed for user'
        r'|sshd.*: authentication failure'
        r'|sshd.*: Failed password for'
        r'|sshd.*- POSSIBLE BREAK-IN ATTEMPT'
        # zuul random test
        r'|zuul.*echo BECOME-SUCCESS-'
        r'|^[^ ]{64}$'
        # useless debug statement
        r'|ovs-ofctl .* (dump-ports|dump-flows|show)\b'
        r'|(ip|eb)tables .* -L\b'
    )
    # See https://en.wikipedia.org/wiki/Percent-encoding
    uri_percent_re = re.compile(r'%[2345][0-9A-F]')
    ip_re = re.compile(r'%s|%s|%s' % (IPV4_RE, IPV6_RE, MAC_RE))
    # For some unknown reason, '_' in (?=) doesn't work in prefix match
    #  re.sub(r'(?=\b|_)test(?=\b|_)',   'RNG', 'AUTH_test_')  -> doesn't work
    #  re.sub(r'(?=\b|_)_?test(?=\b|_)', 'RNG', 'AUTH_test_')  -> works
    power2_re = re.compile(r'(?=\b|_)_?(?:[\w+/]{128}|[\w+/]{64}|'
                           r'[0-9a-fA-F]{40}|[0-9a-fA-F]{32})(?=\b|_)')
    uuid_re = re.compile(r'(?=\b|_)_?(?:%s|tx[^ ]{32})(?=\b|_)' % UUID_RE, re.I)
    date_re = re.compile(r'\b(?:%s|%s|%s|%s)\b' % (DAYS, SHORT_DAYS,
                                                   SHORT_MONTHS, MONTHS), re.I)
    heat_re = re.compile(r'-\w{12}[- \"$]')
    comments = re.compile(r'(?:[\s]*# |^%% |^#|^[\s]*id = ").*')
    alpha_re = re.compile(r'[^a-zA-Z_\/\s]+')
    gitver_re = re.compile(r'git\w+')
    digits_re = re.compile(r'0x[0-9a-fA-F]{2,}|[0-9]+(?:\.\d+)?')
    randpath_re = re.compile(r'(?:/tmp/ansible\.\w{8}'
                             r'|/tmp/tmp\w{6}'
                             r'|/tmp/tmp\.\w{10})\b')
    gitsha_re = re.compile(r'\b\w{7}\.\.\w{7}\b')
    hash_re = re.compile(r'SHA256:[\w+/]{43}\b')

    @staticmethod
    def process(line):
        # Ignore some raw pattern first
        if Tokenizer.rawline_re.search(line):
            return ''
        strip = line
        # Break URI percent encoding
        strip = Tokenizer.uri_percent_re.sub(" ", strip)
        # Remove words that are exactly 32, 64 or 128 character longs
        strip = Tokenizer.power2_re.sub("RNGN", strip)
        # Remove uuid
        strip = Tokenizer.uuid_re.sub("RNGU", strip)
        # Remove heat short uuid but keep spacing
        #  ObjectName-2kbhkd45kcs3-ServiceName -> ObjectName-HEATID-ServiceName
        strip = Tokenizer.heat_re.sub(" HEATID ", strip)
        # Remove git sha
        strip = Tokenizer.gitsha_re.sub("RNGG", strip)
        # Remove hashes
        strip = Tokenizer.hash_re.sub("RNGH", strip)
        # Remove random path
        strip = Tokenizer.randpath_re.sub("RNGP", strip)
        # Remove date
        strip = Tokenizer.date_re.sub("DATE", strip)
        # Remove ip/addr
        strip = Tokenizer.ip_re.sub("RNGI", strip)
        # Remove numbers
        strip = Tokenizer.digits_re.sub("", strip)
        # Only keep characters
        strip = Tokenizer.alpha_re.sub(" ", strip)
        # Remove tiny words
        strip = " ".join(filter(lambda x: len(x) > 3, strip.split()))
        # Weight failure token
        for token in ("error", "fail", "warn"):
            if token in strip.lower():
                strip += " %sA %sB %sC %sD" % (token, token, token, token)
        return strip


def remove_ansible_std_lines_lists(line):
    """Remove stdout_lines: [] list while taking into account nested run"""
    for i in ("stdout", "stderr"):
        token = '"%s_lines": ' % i
        if '"%s": ' % i in line and token in line:
            start_pos = line.index(token)
            pos = start_pos + len(token)
            if line[pos:].startswith('[]'):
                # Nothing to remove
                continue
            # Sanity check
            if not line[pos:].startswith('["'):
                print("Ooops: couldn't find %s beginning '[' in %s" % (token,
                                                                       line))
                return line
            quote = False
            escape = False
            while pos < len(line):
                if not escape:
                    if line[pos] == '"':
                        quote = not quote
                    if not quote and line[pos] == ']':
                        break
                if line[pos] == "\\":
                    escape = True
                else:
                    escape = False
                pos += 1
            if pos == len(line):
                # Ooops
                print("Ooops: couldn't find %s ending ']' in %s" % (token,
                                                                    line))
                return line
            line = line[:start_pos] + line[pos:]
            line.replace('"%s": ' % token, r"\n")
    return line
