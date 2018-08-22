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
import pkg_resources

LOGO = """
iVBORw0KGgoAAAANSUhEUgAAABcAAAAXBAMAAAASBMmTAAAAFVBMVEU6feU9geVjl+WMsOUicOXA
z+X///9aF/8vAAAAAXRSTlMAQObYZgAAAAFiS0dEAIgFHUgAAAAJcEhZcwAACxMAAAsTAQCanBgA
AAAHdElNRQfiCAsFFSh04lDsAAAAp0lEQVQY012QXQ7CQAiE158LyKYH6BDf7WAPYEkP4BLvfxVx
WxPjhge+sAwDpfy9oxgfX5hQhbctt0VV4Z1OjTVC+OowtqeNrekH7rO7qq/3/HcAZoIzOZZy5uDt
IrHKlEN0Yg9D9kugQkVbTaDxMkAM9lOJuvWAhinwkR782dVS+iAwjpqzsDlYr7uDcsKPt3TtEc7o
+8Siqeb7doQI9jwFjcv/YcobPpYhOB4CZRcAAAAASUVORK5CYII=
"""

HTML_DOM = """
<!DOCTYPE html>
<html class='layout-pf'>
  <head>
    <title>Logreduce of {target}</title>
    <meta charset='UTF-8'>
    <link rel='stylesheet' type='text/css' href='{ptnfly_css_loc}'>
    <link rel='stylesheet' type='text/css' href='{ptnfly_cssa_loc}'>
    <style>
.loglines {{max-height: 800px; overflow-y: scroll;}}
.list-group-item-container {{overflow: hidden;}}
.ls {{margin-top: 0px; margin-bottom: 10px; border-color: black;}}
#debuginfo {{display: none;}}
    </style>
  </head>
  <body>
    <nav class="navbar navbar-default navbar-pf" role="navigation">
      <div class="navbar-header">
        <img src="data:image/jpeg;base64,{logo}" alt="LogReduce" />
      </div>
      <div class="collapse navbar-collapse navbar-collapse-1">
        <ul class="nav navbar-nav navbar-utility">
          <li><a href="#" id='debugbtn'>Show Debug</a></li>
          <li><a href="https://pypi.org/project/logreduce/" target="_blank">
            Documentation
          </a></li>
          <li><a href="#"><strong>Version</strong> {version}</a></li>
        </ul>
        <ul class="nav navbar-nav navbar-primary">
            <li class="active"><a href="log-classify.html">Report</a></li>
            <li><a href="ara-report/">ARA Records Ansible</a></li>
            <li><a href="./">Job Artifacts</a></li>
        </ul>
      </div>
    </nav>
    <div class="container" style='width: 100%'>
      {body}
    </div>
    <script src='{jquery_loc}'></script>
    <script src='{bootst_loc}/js/bootstrap.min.js'></script>
    <script src='{ptnfly_loc}/js/patternfly.min.js'></script>
    <script>{js}</script>
  </body>
</html>
"""

JS = """
$(document).ready(function(){
$('#debugbtn').on('click', function(event) {$('[id=debuginfo]').toggle();});
});
$(".list-group-item-header").click(function(event){
  if(!$(event.target).is("button, a, input, .fa-ellipsis-v")){
    $(this).find(".fa-angle-right").toggleClass("fa-angle-down")
      .end().parent().toggleClass("list-view-pf-expand-active")
      .find(".list-group-item-container").toggleClass("hidden");
    }
})
$(".list-group-item-container .close").on("click", function (){
  $(this).parent().addClass("hidden")
         .parent().removeClass("list-view-pf-expand-active")
         .find(".fa-angle-right").removeClass("fa-angle-down");
})
"""


def render_unmatch_list(dom, output):
    if output.get("unknown_files"):
        dom.append("<br /><h2>Unmatched file in previous success logs</h2>")
        dom.append("<ul>")
        for fname in output["unknown_files"]:
            dom.append("<li><a href='%s'>%s</a></li>" % (fname[1], fname[0]))
        dom.append("</ul>")


def table(dom, columns, rows):
    dom.append(
        "<div id='debuginfo' style='overflow-x: auto'>"
        "<table style='white-space: nowrap; margin: 0px' "
        "class='table table-condensed table-responsive table-bordered'>"
    )
    if columns:
        dom.append("<thead><tr>")
        for col in columns:
            dom.append("<th>%s</th>" % col)
        dom.append("</tr></thead>")
    dom.append("<tbody>")
    for row in rows:
        if columns and len(row) > len(columns):
            dom.append("<tr id='%s'>" % row.pop())
        else:
            dom.append("<tr>")
        for col in row:
            dom.append("<td>%s</td>" % col)
        dom.append("</tr>")
    dom.append("</tbody></table><br /></div>")


def render_result_info(dom, output):
    rows = []
    if output.get("train_command"):
        rows.append(("Test command", output["train_command"]))
    rows.append(("Command", output["test_command"]))
    rows.append(("Targets", "%s" % " ".join(
        map(html.escape, map(str, output["targets"])))))
    rows.append(("Baselines", "%s" % " ".join(
        map(html.escape, map(str, output["baselines"])))))
    rows.append(("Anomalies count", output["anomalies_count"]))
    rows.append(("Run time", "%.2f seconds" % output["total_time"]))
    rows.append(("Reduction", "%02.2f%% (from %d lines to %d)" % (
        output["reduction"],
        output["testing_lines_count"],
        output["outlier_lines_count"])))
    table(dom, columns=[], rows=rows)


def render_result_table(dom, files_sorted):
    columns = [
        "Anomaly count",
        "Filename",
        "Test time",
        "Model"
    ]
    rows = []
    for filename, data in files_sorted:
        if not data["scores"]:
            continue
        rows.append((
            len(data["scores"]),
            "<a href='#%s'>%s</a> (<a href='%s'>log link</a>)" % (
                filename.replace('/', '_'), filename, data["file_url"]),
            "%.2f sec" % data["test_time"],
            "<a href='#model_%s'>%s</a>" % (data["model"], data["model"])))
    table(dom, columns, rows)


def render_model_table(dom, model_sorted, links):
    columns = [
        "Model", "Train time", "Infos", "Baseline files"
    ]
    rows = []
    for model_name, data in model_sorted:
        rows.append([
            model_name,
            "%.2f sec" % data["train_time"],
            data["info"],
            " ".join(links[model_name]),
            "model_%s" % model_name,
        ])
    table(dom, columns, rows)


def render_logfile(dom, filename, data, source_links, expanded=False):
    lines_dom = []
    last_pos = None
    for idx in range(len(data["scores"])):
        pos, dist = data["scores"][idx]
        line = data["lines"][idx]
        lines_dom.append(
            "<font color='#%02x0000'>%1.3f | %04d: %s</font><br />" % (
                int(255 * dist), dist, pos + 1, html.escape(line)))
        if last_pos and last_pos != pos and pos - last_pos != 1:
            lines_dom.append("<hr class='ls' />")
        last_pos = pos

    expand = " hidden"
    list_expand = ""
    angle = ""
    if expanded:
        expand = ""
        angle = " fa-angle-down"
        list_expand = " list-view-pf-expand-active"
    dom.append("""
    <div class="list-group-item{list_expand}" id='{anchor}'>
      <div class="list-group-item-header">
        <div class="list-view-pf-expand">
          <span class="fa fa-angle-right{angle}"></span>
        </div>
        <div class="list-view-pf-main-info">
          <div class="list-view-pf-left">
            <span class="fa pficon-degraded list-view-pf-icon-sm"></span>
          </div>
          <div class="list-view-pf-body">
            <div class="list-view-pf-description">
              <div class="list-group-item-heading">
                {filename}
              </div>
              <div class="list-group-item-text">
                (<a href="{loglink}">log link</a>)
              </div>
            </div>
            <div class="list-view-pf-additional-info-item" id='debuginfo'>
              <span class="pficon pficon-registry"></span>
              <a href="{model_link}">{model_name}</a> model
            </div>
            <div class="list-view-pf-additional-info-item">
              <span class="fa fa-bug"></span>
              <strong>{anomaly_count}</strong>
            </div>
          </div>
        </div>
      </div>
      <div class="list-group-item-container container-fluid{expand}">
        <div class="close"><span class="pficon pficon-close"></span></div>
        <div id='debuginfo'>baseline samples:<ul>{baselines}</ul></div>
        <div class="loglines">
          {lines}
        </div>
      </div>
    </div>
    """.format(
        lines="\n".join(lines_dom),
        baselines="".join(map(lambda x: "<li>%s</li>" % x, source_links)),
        list_expand=list_expand,
        expand=expand,
        angle=angle,
        anchor=filename.replace('/', '_'),
        model_name=data['model'],
        model_link="#model_%s" % data['model'],
        anomaly_count=len(data["scores"]),
        filename=filename,
        loglink=data['file_url'],))
    return


def render_html(output, static_location=None):
    if static_location:
        jquery_loc = static_location + "/js/jquery.min.js"
        bootst_loc = static_location + "/bootstrap"
        ptnfly_loc = static_location + "/patternfly"
    else:
        jquery_loc = "https://code.jquery.com/jquery-3.3.1.min.js"
        bootst_loc = "https://maxcdn.bootstrapcdn.com/bootstrap/3.3.7"
        ptnfly_loc = "https://cdnjs.cloudflare.com/ajax/libs/patternfly/3.24.0"

    ptnfly_css_loc = "%s/css/patternfly.min.css" % ptnfly_loc
    ptnfly_cssa_loc = "%s/css/patternfly-additions.min.css" % ptnfly_loc

    body = []

    render_result_info(body, output)

    files_sorted = sorted(
        output['files'].items(),
        key=lambda x: (x[0].startswith("job-output.txt") or
                       x[1]['mean_distance']),
        reverse=True)

    models_sorted = sorted(
        output['models'].items(),
        key=lambda x: x[1]['train_time'],
        reverse=True)
    links = {}
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
        links[model_name] = source_links

    render_result_table(body, files_sorted)

    body.append('<div class="list-group list-view-pf list-view-pf-view">')
    first = True
    for filename, data in files_sorted:
        if not data["scores"]:
            continue
        render_logfile(body, filename, data, links[data["model"]], first)
        first = False
    body.append('</div>')

    render_model_table(body, models_sorted, links)

    render_unmatch_list(body, output)

    return HTML_DOM.format(
        target=" ".join(map(html.escape, map(str, output["targets"]))),
        js=JS,
        logo=LOGO.replace('\n', ''),
        version=pkg_resources.get_distribution("logreduce").version,
        body="\n".join(body),
        jquery_loc=jquery_loc,
        bootst_loc=bootst_loc,
        ptnfly_loc=ptnfly_loc,
        ptnfly_css_loc=ptnfly_css_loc,
        ptnfly_cssa_loc=ptnfly_cssa_loc)
