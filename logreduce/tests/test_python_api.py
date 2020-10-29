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

from typing import Callable, List, Tuple
from pathlib import Path
from tempfile import mkdtemp
from shutil import rmtree
from logreduce import Classifier


def setup_tree(base: Path, tree: List[Tuple[str, str]]) -> None:
    """Create file tree on base directory"""
    for rel_path, content in tree:
        path = base / rel_path
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(content)


def run(tree: List[Tuple[str, str]], test: Callable[[Path], bool]) -> bool:
    """Run a test on a file tree"""
    base = Path(mkdtemp())
    try:
        setup_tree(base, tree)
        return test(base)
    finally:
        rmtree(base)
    return False


def test_simple() -> None:
    # This classifier only remove `kubernetes-uuid:` from log file
    clf = Classifier(
        filename_to_modelname=lambda fn: fn,
        keep_file=lambda _: True,
        process_line=lambda x: x.replace("kubernetes-uuid:", ""),
    )
    # Set a tiny threshold as equal content may have a tiny distance like 0.002
    clf.threshold = 0.01

    def validate_ok(base: Path) -> bool:
        # Validate no anomalies are found
        clf.train([str(base / "success")])
        for anomaly in filter(lambda x: x[3] != [], clf.test([str(base / "failure")])):
            print("Found: ", anomaly)
            return False
        return True

    def validate_failure(base: Path) -> bool:
        # The inverse of validate_ok
        return not validate_ok(base)

    # Ensure the process_line correctly remove the kubernetes-uuid
    assert run(
        [
            ("success/test-file", "normal log message"),
            ("failure/test-file", "kubernetes-uuid: normal log message"),
        ],
        validate_ok,
    )

    # Ensure uncaugh uuid are still detected
    assert run(
        [
            ("success/test-file", "normal log message"),
            ("failure/test-file", "kubernetes-uid: normal log message"),
        ],
        validate_failure,
    )
