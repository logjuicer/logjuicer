logreduce - extract anomaly from log files
==========================================

Based on success logs, logreduce highlights useful text in failed logs.
The goal is to save time in finding a failure's root cause.

On average, learning run at 2000 lines per second, and
testing run at 1300 lines per seconds.


How it works
------------

logreduce uses a *model* to learn successful logs and detect novelties in
failed logs:

* Random words are manually removed using regular expression
* Then lines are converted to a matrix of token occurrences
  (using **HashingVectorizer**),
* An unsupervised learner implements neighbor searches
  (using **NearestNeighbors**).


Caveats
-------

This method doesn't work when debug content is only included in failed logs.
To successfully detect anomalies, failed and success logs needs to be similar,
otherwise the extra informations in failed logs will be considered anomalous.

For example this happens with testr where success logs only contains 'SUCCESS'.


Install
-------

* Fedora:

.. code-block:: console

  sudo dnf install -y python3-scikit-learn
  git clone https://softwarefactory-project.io/r/logreduce
  pushd logreduce
  python3 setup.py develop --user
  popd


* openSUSE:

.. code-block:: console

  sudo zypper install python3-scikit-learn
  git clone https://softwarefactory-project.io/r/logreduce
  pushd logreduce
  python3 setup.py develop --user
  popd


* Pip:

.. code-block:: console

  pip install --user logreduce


Usage
-----

Logreduce needs a **baseline** for success log training, and a **target**
for the log to reduce.

Logreduce prints anomalies on the console, the log files are not modified:

.. code-block:: console

  "%(distance)f | %(log_path)s:%(line_number)d: %(log_line)s"

Local file usage
................

* Compare two files or directories without building a model:

.. code-block:: console

  $ logreduce diff testr-nodepool-01/output.good testr-nodepool-01/output.fail
  0.232 | testr-nodepool-01/output.fail:0677:  File "voluptuous/schema_builder.py", line 370, in validate_mapping
  0.462 | testr-nodepool-01/output.fail:0678:    raise er.MultipleInvalid(errors)
  0.650 | testr-nodepool-01/output.fail:0679:  voluptuous.error.MultipleInvalid: required key not provided @ data['providers'][2]['cloud']

* Compare two files or directories:

.. code-block:: console

  $ logreduce dir preprod-logs/ /var/log/


* Or build a model first and run it separately:

.. code-block:: console

  $ logreduce dir-train sosreport.clf old-sosreport/ good-sosreport/
  $ logreduce dir-run sosreport.clf new-sosreport/


Zuul job usage
..............

Logreduce can query Zuul build database to train a model.

* Extract novelty from a job logs:

.. code-block:: console

  $ logreduce job http://logs.openstack.org/...

  # Reduce comparaison to a single project (e.g. for tox jobs)
  $ logreduce job --project openstack/nova http://logs.openstack.org/...

  # Compare using many baselines
  $ logreduce job --count 10 http://logs.openstack.org/...

  # Include job artifacts
  $ logreduce job --include-path logs/ http:/logs.openstack.org/...

* Or build a model first and run it separately:

.. code-block:: console

  $ logreduce job-train --job job_name job_name.clf
  $ logreduce job-run job_name.clf http://logs.openstack.org/.../


Journald usage
..............

Logreduce can look for anomaly in journald, comparing the last day/week/month
to the previous one:

* Extract novelty from last day journal:

.. code-block:: console

  $ logreduce journal --range day

* Build a model using journal of last month and look for novelty in last week:

.. code-block:: console

  $ logreduce journal-train --range month good-journal.clf
  $ logreduce journal-run --range week good-journal.clf


Filters configuration
.....................

Some content yields false positives that can be ignored through filters.
Using the `--config` command line attribute, filters can be set for
exclude_files, exclude_paths and exclude_lines. Here is an example
filters configuration file:

.. code-block:: yaml

   filters:
     exclude_files:
       - "deployment-hieradata.j2.yaml"
       - "tempest.html"
     exclude_paths:
       - "group_vars/Compute"
       - "group_vars/Controller"
       - "group_vars/Undercloud"
     exclude_lines:
       # neutron dhcp interface
       - "^tap[^ ]*$"
       # IPA cookies
       - "^.*[Cc]ookie.*ipa_session="


Server component (Experimental)
-------------------------------

A server component may be deployed to build an anomaly database and produce
dataset. This initial implementation is focused on Zuul builds and it
doesn't support importing arbritary files yet.
More details in this specification https://review.openstack.org/#/c/581214:

Components list:

* logreduce-server: the REST and Gearman server
* logreduce-worker: job executor
* logreduce-client: client cli
* logreduce-webui: logreduce web interface

API
...

* PUT /anomaly/new: receive user report request from os_loganalyze
* PUT /anomaly: import an anomaly report (json file generated by standalone cli)
* GET /anomaly/{anomaly_id}: return an anomaly details
* POST /anomaly/{anomaly_id}: update anomaly status
* POST /anomaly/{anomaly_id}/logfile/{logfile_id}: update scores
* DELETE /anomaly/{anomaly_id}/logfile/{logfile_id}: remove a file
* GET /anomalies: return the list of anomalies
* GET /status: return the list of worker jobs


Installation
............

Here is a brief documentation to setup the server components:

.. code-block:: console

   # Setup service
   useradd -m -d /var/lib/logreduce -c "Logreduce Daemon" logreduce
   mkdir /etc/logreduce /var/log/logreduce
   chown -R logreduce /var/log/logreduce

   # Install extra requirements
   sudo -u logreduce pip3 install --user "logreduce[server]"

   # Copy configuration
   cp etc/logreduce/config.yaml /etc/logreduce/
   cp etc/httpd/log-classify.conf /etc/httpd/conf.d/
   cp etc/systemd/*.service /lib/systemd/system
   chown -R logreduce /var/log/logreduce

   # Update configuration
   # Edit public_url with the http server public address

   # Start the service
   systemctl enable logreduce-*
   systemctl start logreduce-*
   systemctl status logreduce-*
   # -> status should display 'INFO logreduce.ServerWorker: Connected'
   # -> /var/log/logreduce/worker.log should say 'Starting RPC listener'

   # Build and install the web interface
   cd web
   yarn install
   yarn build
   rsync -a build/ /usr/share/log-classify/
   # -> curl localhost/log-classify should return the index.html


logreduce-tests
---------------

This package contains tests data for different type of log such as testr
or syslog. Each tests includes a pre-computed list of the anomalies in log
failures.

This package also includes a command line utility to run logreduce against all
tests data and print a summary of its performance.


Test format
...........

Each tests case is composed of:

* A *.good* file (or directory) that holds the baseline
* A *.fail* file (or directory)
* A *info.yaml* file that describe expected output:

.. code-block:: yaml

  threshold: float # set the distance threshold for the test
  anomalies:
    - optional: bool  # to define minor anomalies not considered false positive
      lines: |        # the expected lines to be highlighted
        Traceback...
        RuntimeError...


Evaluate
........

To run the evaluation, first install logreduce-tests:

.. code-block:: console

  git clone https://softwarefactory-project.io/r/logreduce-tests
  pushd logreduce-tests
  python3 setup.py develop --user

logreduce-tests expect tests directories as argument:

.. code-block:: console

  $ logreduce-tests tests/testr-zuul-[0-9]*
  [testr-zuul-01]: 100.00% accuracy,  5.00% false-positive
  [testr-zuul-02]:  80.00% accuracy,  0.00% false-positive
  ...
  Summary:  90.00% accuracy,  2.50% false-positive

Add --debug to display false positive and missing chunks.


TODOs
-----

* Add terminal colors output
* Add progress bar
* Better differentiate training debug from testing debug
* Add a starting log line and report written
* Add tarball traversal in utils.files_iterator
* Add logstash filter module
* Improve tokenization tests


Roadmap
-------
* Add daemon worker mode with MQTT event listener
* Discard files that are 100% anomalous
* Report mean diviation instead of absolute distances
* Investigate second stage model


Contribute
----------

Contribution are most welcome, use **git-review** to propose a change.
Setup your ssh keys after sign in https://softwarefactory-project.io/auth/login
