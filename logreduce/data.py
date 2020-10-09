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

"""
Logreduce data type definitions

The TypedDict are used as a transition to proper dataclass.
"""

from typing import Any, Dict, List, Tuple, Union

try:
    # Python <3.8
    from typing_extensions import TypedDict
except ImportError:
    from typing import TypedDict  # type: ignore

Build = TypedDict(
    "Build",
    {"log_url": str, "local_path": str, "uuid": str, "ref": str, "project": str},
)


def show_build(build: Build) -> str:
    inf = "id=%s ref=%s" % (build["uuid"][:7], build["ref"])
    if build.get("project"):
        inf += " project=%s" % build["project"]
    if build.get("local_path"):
        inf += " local_path=%s" % build["local_path"]
    if build.get("log_url"):
        inf += " log_url=%s" % build["log_url"]
    return "<ZuulBuild %s>" % inf


class FileLike:
    def __next__(self) -> bytes:
        pass

    def __str__(self) -> str:
        pass

    def open(self) -> None:
        pass

    def close(self) -> None:
        pass

    def __iter__(self) -> "FileLike":
        return self


# Logreduce inputs can be a path or a build
LogObject = Union[str, Build, FileLike]


def show_logobject(obj: LogObject) -> str:
    if isinstance(obj, dict):
        return show_build(obj)
    return str(obj)


Result = TypedDict(
    "Result",
    {
        "files": Dict[str, Any],
        "models": Dict[str, Any],
        "targets": List[Any],
        "baselines": List[LogObject],
        "train_command": str,
        "test_command": str,
        "anomalies_count": int,
        "total_time": float,
        "reduction": float,
        "training_lines_count": int,
        "testing_lines_count": int,
        "outlier_lines_count": int,
        "unknown_files": List[Tuple[str, str]],
    },
)
