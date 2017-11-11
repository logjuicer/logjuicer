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
RANDOM_PREFIXES = r'tmp\.|tmp|br|tap|qdhcp-|req-|ns-|0x|a[0-9]+='
RANDOM_DIRS = r'ansible_|omit_place_holder__|instack\.|dib_build\.'
MIXED_ALPHA_DIGITS_WORDS = r'[a-z0-9]*[0-9][a-z0-9]*'
UUID_RE = r'[0-9a-f]{8}-?[0-9a-f]{4}-?4[0-9a-f]{3}-?[89ab][0-9a-f]{3}-' \
          '?[0-9a-f]{12}'


class Tokenizer:
    rawline_re = re.compile(
        r'('
        # useless http GET
        r'"GET / HTTP/1.1" 200'
        r'|"OPTIONS * HTTP/1.0" 200'
        # ssh keys
        r'|AAAA[A-Z][0-9]'
        # hashed password
        r'|\$[0-9]\$'
        # git status
        r'|HEAD is now at|[a-z0-9]{40}|Change-Id: '
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
        # zuul random test
        r'|zuul.*echo BECOME-SUCCESS-'
        r')')
    power2_re = re.compile(r'([0-9a-f]{32}|[0-9a-f]{64}|[0-9a-f]{128})', re.I)
    randword_re = re.compile(r'\b(' +
                             r'%s' % DAYS +
                             r'|%s' % SHORT_MONTHS +
                             r'|%s' % MONTHS +
                             r'|%s' % RANDOM_PREFIXES +
                             r'|%s' % RANDOM_DIRS +
                             r'|%s' % UUID_RE +
                             r'|%s' % MIXED_ALPHA_DIGITS_WORDS +
                             r')[^\s\.\/]*', re.I)
    comments = re.compile(r'([\s]*# |^%% |^#|^[\s]*id = ").*')
    alpha_re = re.compile(r'[^a-zA-Z_\/\s]')
    gitver_re = re.compile(r'git[a-z0-9]+', re.I)

    @staticmethod
    def process(line):
        """Extract interesing part"""
        # Ignore some raw pattern first
        if Tokenizer.rawline_re.search(line):
            return ''
        strip = line
        # Remove words that are exactly 32, 64 or 128 character longs
        strip = Tokenizer.power2_re.subn("", strip)[0]
        # Remove git version
        strip = Tokenizer.gitver_re.subn("", strip)[0]
        # Remove comments that are stripped when a file is formated
        strip = Tokenizer.comments.subn("", strip)[0]
        # Remove known random word
        strip = Tokenizer.randword_re.subn("", strip)[0]
        # Only keep characters
        strip = Tokenizer.alpha_re.subn(" ", strip)[0]
        # Remove tiny words
        strip = " ".join(filter(lambda x: len(x) > 3, strip.split()))
        # Ignore line that are too small
        if len(strip) < 7 or ' ' not in strip:
            return ''
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
