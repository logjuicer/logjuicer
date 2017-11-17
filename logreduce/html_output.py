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
import os.path
import sys


def render_html(output):
    dom = ["<html><head>"
           "<title>Logreduce of %s</title>"
           "<link rel='stylesheet' href='bootstrap.min.css'>"
           "<script src='bootstrap.min.js'></script>"
           "<style>.panel-body {max-height: 800; overflow-y: scroll;}</style>"
           "</head><body style='margin-left: 20px'>"
           "<h2>Logreduce</h2>" % " ".join(output["target"])]
    # Results info
    dom.append("<ul>")
    dom.append("  <li>Command: %s</li>" % " ".join(sys.argv))
    dom.append("  <li>Target: %s</li>" % " ".join(output["target"]))
    dom.append("  <li>Baseline: %s</li>" % " ".join(output["baseline"]))
    dom.append("  <li>Run time: %.2f seconds</li>" % output["total_time"])
    dom.append("  <li>%02.2f%% reduction (from %d lines to %d)</li>" % (
        output["reduction"],
        output["testing_lines_count"],
        output["outlier_lines_count"]))
    dom.append("</ul>")
    # Results table of content
    dom.append("<div style='overflow-x: scroll'>"
               "<table style='white-space: nowrap; margin: 0px' "
               "class='table table-condensed table-responsive'>"
               "<thead><tr>"
               "<th>Count</th><th>Filename</th><th>Compared too</th>"
               "</tr></thead><tbody>")
    files_sorted = sorted(output['files'].items(),
                          key=lambda x: x[1]['mean_distance'],
                          reverse=True)
    for filename, data in files_sorted:
        if not data["chunks"]:
            continue
        source_links = []
        for source_file in data["source_files"]:
            if source_file.startswith("http"):
                source_dir = os.path.basename(os.path.dirname(source_file))
                if source_dir.startswith("Z"):
                    source_dir = ""
                source_links.append("<a href='%s'>%s</a>" % (
                    source_file, os.path.join(source_dir,
                                              os.path.basename(source_file))
                ))
            else:
                source_links.append(source_file)
        dom.append("  <tr>"
                   "<td>%d</td>" % len(data["scores"]) +
                   "<td><a href='#%s'>%s</a> (<a href='%s'>link</a>)</td>" % (
                       filename.replace('/', '_'), filename,
                       data["file_url"]) +
                   "<td>%s</td>" % " ".join(source_links) +
                   "</tr>")
    dom.append("</tbody></table></div><br />")

    for filename, data in files_sorted:
        if not data["chunks"]:
            continue
        dom.append(
            "<div class='panel panel-default' id='%s'>" % (
                filename.replace('/', '_')) +
            "<div class='panel-heading'><a href='%s'>%s</a></div>" % (
                data['file_url'], filename) +
            "<div class='panel-body'>")
        for idx in range(len(data["chunks"])):
            lines = data["chunks"][idx].split('\n')
            for line_pos in range(len(lines)):
                line_score = data["scores"][idx][line_pos]
                dom.append(
                    "<font color='#%02x0000'>%1.3f | %04d: %s</font><br />" % (
                        int(255 * line_score),
                        line_score,
                        data["line_pos"][idx][line_pos],
                        html.escape(lines[line_pos])))
            dom.append("<hr style='margin-top: 0px; margin-bottom: 10px;' />")
        dom.append("</div></div>")

    if output.get("unknown_files"):
        dom.append("<br /><h2>Unmatched file in previous success logs</h2>")
        dom.append("<ul>")
        for fname in output["unknown_files"]:
            dom.append("<li><a href='%s'>%s</a></li>" % (fname[1], fname[0]))
        dom.append("</ul>")
    dom.append("</body></html>")
    return "\n".join(dom)
