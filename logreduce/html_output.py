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
           "<link rel='stylesheet' "
           "href='/static/bootstrap/css/bootstrap.min.css'>"
           "<script src='/static/js/jquery.min.js'></script>"
           "<script src='/static/bootstrap/js/bootstrap.min.js'></script>"
           "<script>$(document).ready(function(){"
           "$('#debugbtn').on('click', function(event) {\n"
           "$('[id=debuginfo]').toggle();\n"
           "});});"
           "</script>"
           "<style>.panel-body {max-height: 800; overflow-y: scroll;}\n"
           "#debuginfo {display: none;}</style>"
           "</head><body style='margin-left: 20px'>"
           "<h2 id='debuginfo'>Logreduce</h2>" %
           " ".join(output["target"])]
    dom.append("<button type='button' id='debugbtn' "
               "class='pull-right btn-xs btn-primary btn'>Show Debug</button>")
    # Results info
    dom.append("<ul id='debuginfo'>")
    dom.append("  <li>Command: %s</li>" % " ".join(sys.argv))
    dom.append("  <li>Target: %s</li>" % " ".join(output["target"]))
    dom.append("  <li>Baseline: %s</li>" % " ".join(output["baseline"]))
    dom.append("  <li>Anomalies count: %d</li>" % output["anomalies_count"])
    dom.append("  <li>Run time: %.2f seconds</li>" % output["total_time"])
    dom.append("  <li>%02.2f%% reduction (from %d lines to %d)</li>" % (
        output["reduction"],
        output["testing_lines_count"],
        output["outlier_lines_count"]))
    dom.append("</ul>")
    # Results table of content
    dom.append("<div id='debuginfo' style='overflow-x: scroll'>"
               "<table style='white-space: nowrap; margin: 0px' "
               "class='table table-condensed table-responsive'>"
               "<thead><tr>"
               "<th>Anomaly count</th><th>Filename</th>"
               "<th>Test time</th><th>Model</th>"
               "</tr></thead><tbody>")
    files_sorted = sorted(output['files'].items(),
                          key=lambda x: x[1]['mean_distance'],
                          reverse=True)
    for filename, data in files_sorted:
        if not data["chunks"]:
            continue
        dom.append("  <tr>"
                   "<td>%d</td>" % len(data["scores"]) +
                   "<td><a href='#%s'>%s</a> (<a href='%s'>log link</a>)</td>"
                   % (filename.replace('/', '_'), filename,
                      data["file_url"]) +
                   "<td>%.2f sec</td>" % data["test_time"] +
                   "<td><a href='#model_%s'>%s</a></td>" % (
                       data["model"], data["model"]) +
                   "</tr>")
    dom.append("</tbody></table></div><br />")

    # Model table
    model_dom = [
        "<div id='debuginfo' style='overflow-x: scroll'>"
        "<table style='white-space: nowrap; margin: 0px' "
        "class='table table-condensed table-responsive'>"
        "<thead><tr>"
        "<th>Model</th><th>Train time</th>"
        "<th>Infos</th><th>Baseline files</th>"
        "</tr></thead><tbody>"]
    models_sorted = sorted(output['models'].items(),
                           key=lambda x: x[1]['train_time'],
                           reverse=True)
    for model_name, data in models_sorted:
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
        data["source_links"] = source_links
        model_dom.append("  <tr id='model_%s'>" % model_name +
                         "<td>%s</td>" % model_name +
                         "<td>%.2f sec</td>" % data["train_time"] +
                         "<td>%s</td>" % data["info"] +
                         "<td>%s</td>" % " ".join(data["source_links"]) +
                         "</tr>")
    model_dom.append("</tbody></table></div><br />")

    # Anomalies result table
    for filename, data in files_sorted:
        if not data["chunks"]:
            continue
        heading_dom = (
            "<div class='panel-heading'>"
            "%s (<a href='%s'>log link</a>)"
            "<span class='pull-right' id='debuginfo'>model: "
            "<a href='#model_%s'>%s</a> (%s)"
            "</span></div>" % (
                filename, data['file_url'],
                data['model'], data['model'],
                output["models"][data["model"]]["info"]))

        dom.append(
            "<div class='panel panel-default' id='%s'>" % (
                filename.replace('/', '_')) +
            heading_dom +
            "<div class='panel-body'>")
        # Link sample baseline
        dom.append("<div id='debuginfo'>baseline samples:<ul>")
        for source_link in output["models"][data["model"]]["source_links"]:
            dom.append("<li>%s</li>" % source_link)
        dom.append("</ul></div>")
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
            if idx < len(data["chunks"]) - 1:
                dom.append("<hr style='margin-top: 0px; margin-bottom: 10px; "
                           "border-color: black;' />")
        dom.append("</div></div>")

    dom.extend(model_dom)
    if output.get("unknown_files"):
        dom.append("<br /><h2>Unmatched file in previous success logs</h2>")
        dom.append("<ul>")
        for fname in output["unknown_files"]:
            dom.append("<li><a href='%s'>%s</a></li>" % (fname[1], fname[0]))
        dom.append("</ul>")
    dom.append("<h4>--&gt; <a href='./'>Full logs</a> // "
               "<a href='ara'>ARA Record Ansible</a> &lt;--</h4>")
    dom.append("</body></html>")
    return "\n".join(dom)
