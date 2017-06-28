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

import html


def render_html(output):
    dom = ["<html><head><title>Logreduce of %s</title></head><body>" % output["target"]]
    # Results info
    dom.append("<ul><li>Target: %s</li>" % output["target"] +
               "<li>Baseline: %s</li>" % output["baseline"] +
               "<li>Run time: %.2f seconds</li>" % output["total_time"] +
               "<li>%02.2f%% reduction (from %d lines to %d)</li>" % (output["reduction"], output["testing_lines_count"], output["outlier_lines_count"]) +
               "</ul>")
    # Results table of content
    dom.append("<table><thead><tr><th>Filename</th><th>Compared too</th><th>Number</th></tr></thead>")
    for filename, data in output["files"].items():
        if not data["chunks"]:
            continue
        dom.append("  <tr><td><a href='#%s'>%s</a></td><td>%s</td><td>%d</td></tr>" % (
            filename.replace('/', '_'), filename, " ".join(data["source_files"]), len(data["scores"])
        ))
    dom.append("</table>")

    for filename, data in output["files"].items():
        if not data["chunks"]:
            continue
        dom.append("<span id='%s'><h3>%s</h3>" % (filename.replace('/', '_'), filename))
        for idx in range(len(data["chunks"])):
            lines = data["chunks"][idx].split('\n')
            for line_pos in range(len(lines)):
                line_score = data["scores"][idx][line_pos]
                dom.append("<font color='#%02x0000'>%1.3f | %04d: %s</font><br />" % (
                    int(255 * line_score), line_score, data["line_pos"][idx][line_pos], html.escape(lines[line_pos])))
            dom.append("<hr />")
        dom.append("</span>")

    dom.append("</body></html>")
    return "\n".join(dom)
