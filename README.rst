logreduce - extract anomaly from log files
==========================================

Based on success logs, logreduce highlights useful text in failed logs.
The goal is to save time in finding a failure's root cause.

On average, learning run at 1000 lines per second, and
testing run at 0.800k lines per seconds.


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

* Pip:

.. code-block:: console

  pip install --user logreduce


Usage
-----

Logreduce needs a **baseline** for success log training, and a **target**
for the log to reduce.

Logreduce prints anomalies on the console, the log files are not modified:

.. code-block:: console

  # "%(distance)f | %(log_path)s:%(line_number)d: %(log_line)s"

  $ logreduce diff testr-nodepool-01/output.good testr-nodepool-01/output.fail
  [...]
  0.232 | testr-nodepool-01/output.fail:0677:	  File "voluptuous/schema_builder.py", line 370, in validate_mapping
  0.462 | testr-nodepool-01/output.fail:0678:	    raise er.MultipleInvalid(errors)
  0.650 | testr-nodepool-01/output.fail:0679:	voluptuous.error.MultipleInvalid: required key not provided @ data['providers'][2]['cloud']


Examples
--------

* Look for new events in log files:

.. code-block:: console

  $ logreduce diff /var/log/audit/audit.log.4 /var/log/audit/audit.log --context-length 0
  0.276 | /var/log/audit/audit.log:0606: type=USER_AUTH msg=audit(1498373150.931:1661763): pid=20252 uid=0 auid=1000 ses=19490 subj=unconfined_u:unconfined_r:unconfined_t:s0-s0:c0.c1023 msg='op=PAM:authentication grantors=pam_rootok acct="root" exe="/usr/bin/su" hostname=? addr=? terminal=pts/0 res=success'
  0.287 | /var/log/audit/audit.log:0607: type=USER_ACCT msg=audit(1498373150.931:1661764): pid=20252 uid=0 auid=1000 ses=19490 subj=unconfined_u:unconfined_r:unconfined_t:s0-s0:c0.c1023 msg='op=PAM:accounting grantors=pam_succeed_if acct="root" exe="/usr/bin/su" hostname=? addr=? terminal=pts/0 res=success'

* Look for anomaly in Zuul jobs:

.. code-block:: console

  $ logreduce logs --zuul-web http://zuul.openstack.org --project openstack-infra/zuul --threshold 0.5 --context-length 0 \
       http://logs.openstack.org/64/544964/3/check/tox-pep8/05fc35f/
  0.592 | job-output.txt.gz:0518: 2018-02-22 11:22:10.888593 | ubuntu-xenial | ./tests/unit/test_merger_repo.py:81:9: F841 local variable 'remote_sha' is assigned to but never used
  2018-02-28 09:17:13,886 INFO  Classifier - Testing took 0.068s at 0.767MB/s (9.962kl/s) (0.052 MB - 0.680 kilo-lines)
  99.85% reduction (from 680 lines to 1)

* Look for anomaly in Zuul jobs artifacts:

.. code-block:: console

  $ logreduce logs --zuul-web http://zuul.openstack.org --threshold 0.5 --include-path controller/ --exclude-file unbound_log.txt --pipeline check \
       http://logs.openstack.org/34/548134/1/check/nodepool-functional-py35/3aab684/
  0.500 | job-output.txt.gz:0634: 2018-02-27 03:29:38.186475 | controller |   "ephemeral_device": "VARIABLE IS NOT DEFINED!"
  0.711 | controller/logs/libvirt/libvirtd_log.txt.gz:0536:       2018-02-27 03:37:12.853+0000: 24202: debug : virPCIDeviceFindCapabilityOffset:541 : 1af4 1000 0000:00:03.0: failed to find cap 0x10
  2018-02-28 09:37:13,102 INFO  Classifier - Testing took 49.910s at 0.405MB/s (2.380kl/s) (20.207 MB - 118.798 kilo-lines)
  99.97% reduction (from 118798 lines to 35)


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


Roadmap/todo
------------

* Add logstash filter module
* Add daemon worker mode with MQTT event listener
* Add tarball traversal in utils.files_iterator
* Improve tokenization tests
* Discard files that are 100% anomalous
* Report mean diviation instead of absolute distances


Contribute
----------

Contribution are most welcome, use **git-review** to propose a change.
Setup your ssh keys after sign in https://softwarefactory-project.io/auth/login
