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

from typing import Any, Dict, List, Tuple

try:
    # Python <3.8
    from typing_extensions import TypedDict
except ImportError:
    from typing import TypedDict  # type: ignore

Result = TypedDict(
    "Result",
    {
        "files": Dict[str, Any],
        "models": Dict[str, Any],
        "targets": List[Any],
        "baselines": List[Any],
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
