logreduce - extract anomaly from log files
==========================================

Based on success logs, logreduce highlights useful text in failed logs.
The goal is to assist in debugging failure and reduce effort needed to read
boring log files.

On average, learning run at 10k lines per second, and
Testing run at 0.3k lines per seconds.


How it works
------------

logreduce uses a *model* to learn successful logs and detect outliers in
failed log that doesn't fit the model. The model is constructed as follow:

* Random words are manually removed using regular expression
* Striped lines are converted into a dictionary based numeric vector
  (using **CountVectorizer**),
* Vector are weighted based on term frequencies times inverse
  document-frequency (using **TfidfTransformer**),
* Then vector are used in a nearest neighbor approximator called Locality Sensitive
  Hashing Forest (using **LSHForest**).

In short, logreduce relies heavily on line stripping with a bag-of-words
technique and it uses the distance to known sentence to detect outliers.

For example this input:

.. code-block:: console

  2017-06-21 04:37:45,827 INFO [nodepool.builder.UploadWorker.0] Uploading DIB image build 0000000002 from /tmpxvLOTg/fake-image-0000000002.qcow2 to fake-provider

Results in:

.. code-block:: console

  INFO nodepool builder UploadWorker Uploading image build from /fake image fake provider


The tokenization makes the model a bit dependent on the target data, for example,
to support OpenStack logs, words begining by ns- or req- are taken into account.
Further improvement such as characters n-gram may remove such limitation.


Install
-------

* Fedora:

.. code-block:: console

  sudo dnf install -y python3-scikit-learn lftp
  git clone https://softwarefactory-project.io/r/logreduce
  pushd logreduce
  sudo python3 setup.py develop
  popd

* Pip:

.. code-block:: console

  pip install --user logreduce


Usage
-----

Log files can be:

* A single file
* A directory
* A jenkins job name

Logreduce needs a **--baseline** for success log training, and a **target**
for the log to reduce.

Logreduce will print anomalies on the console, the log files are not modified.
When using the text **--output-format**, anomalies are printed using this format:

.. code-block:: console

  # "%(distance)f | %(log_path)s:%(line_number)d: %(log_line)s"

  $ logreduce --baseline testr-nodepool-01/output.good testr-nodepool-01/output.fail
  [...]
  0.232 | testr-nodepool-01/output.fail:0677:	  File "voluptuous/schema_builder.py", line 370, in validate_mapping
  0.462 | testr-nodepool-01/output.fail:0678:	    raise er.MultipleInvalid(errors)
  0.650 | testr-nodepool-01/output.fail:0679:	voluptuous.error.MultipleInvalid: required key not provided @ data['providers'][2]['cloud']

When using jenkins, the log syntax is *jenkins*:*job-name*[:*job-number*].
When job-number is omited, logreduce automatically uses the lastSuccessfulBuild as baseline
and the lastFailedBuild for the target.

The model can be trained and saved for re-use using **--save**.
When using **--load** logreduce doesn't need a **--baseline**.

Full usage:

.. code-block:: console

  $ usage: logreduce [-h] [--debug] [--debug-token]
                   [--output-format {text,json,yaml,pprint,html}] [--save FILE]
                   [--load FILE] [--jenkins-url JENKINS_URL] [--fetch-artifacts]
                   [--threshold THRESHOLD] [--merge-distance MERGE_DISTANCE]
                   [--before-context BEFORE_CONTEXT]
                   [--after-context AFTER_CONTEXT] [--baseline LOG]
                   [target [target ...]]

  positional arguments:
    target                The log to reduce

  optional arguments:
    -h, --help            show this help message and exit
    --debug               Print debug
    --debug-token         Print tokenization process
    --output-format {text,json,yaml,pprint,html}
    --save FILE           Save the model
    --load FILE           Load a previous model
    --jenkins-url JENKINS_URL
                          Target a custom Jenkins service
    --fetch-artifacts     Fetch zuul-swift-upload artifacts (needs lftp)
    --threshold THRESHOLD
                          Outlier distance threshold, set to 0.0 to display all
                          log, 1.0 to only display clear anomalies
    --merge-distance MERGE_DISTANCE
                          Distance between chunks to merge in a continuous one
    --before-context BEFORE_CONTEXT
                          Amount of lines to include before the anomaly
    --after-context AFTER_CONTEXT
                          Amount of lines to include after the anomaly
    --baseline LOG        A success log


See bellow for some examples


Examples
--------

* Look for anomalies in a flaky jenkins jobs. The DLRN-rpmbuild is used by
  different projects, thus the output varies even between successful jobs.
  In this case we can uses the **--threshold** parameter to reduces false-positive:

.. code-block:: console

  $ logreduce --baseline jenkins:DLRN-rpmbuild --threshold 0.4 --jenkins-url https://review.rdoproject.org/jenkins
  [...]
  0.425 | DLRN-rpmbuild/12483/console:7530: 2017-06-24 13:36:02,886 INFO:dlrn-build:DEBUG: IOError: [Errno 2] No such file or directory: u'/builddir/build/BUILD/python-openstackclient-3.11.1.dev52/man/.doctrees/man/openstack.doctree'
  0.731 | DLRN-rpmbuild/12483/console:7535: 2017-06-24 13:36:02,950 INFO:dlrn-build:DEBUG: error: Bad exit status from /var/tmp/rpm-tmp.rhaVaW (%install)

  # -> Reduced 7654 lines to 71

* Look for anomalies in a job artifacts:

.. code-block:: console

  $ logreduce  --baseline jenkins:gate-weirdo-dlrn-master-puppet-scenario001:804 \
                          jenkins:gate-weirdo-dlrn-master-puppet-scenario001:805 \
               --threshold 0.7 --jenkins-url https://review.rdoproject.org/jenkins
  [...]
  0.935 | scenario001/805/console:1460: AssertionError: From test "assert no delete metrics have the gabbilive policy" :
  0.813 | scenario001/805/console:1479:   "message": "The request you have made requires authentication.",

  # -> Reduced 3475 lines to 34
  # Re-run above command with --fetch-artifacts

  $ logreduce  --baseline jenkins:gate-weirdo-dlrn-master-puppet-scenario001:804 \
                          jenkins:gate-weirdo-dlrn-master-puppet-scenario001:805 \
               --threshold 0.7 --jenkins-url https://review.rdoproject.org/jenkins \
	       --fetch-artifacts
  [...]
  0.736 | scenario001/805/artifacts/artifacts/weirdo-project/logs/aodh/evaluator.txt.gz:0205:      2017-06-20 09:34:56.710 32167 ERROR aodh.evaluator.threshold EndpointNotFound: public endpoint for metering service in RegionOne region not found
  0.893 | scenario001/805/artifacts/artifacts/weirdo-project/logs/keystone/keystone.txt.gz:0082:   2017-06-20 09:01:04.573 31269 ERROR keystone OperationalError: (pymysql.err.OperationalError) (1045, u"Access denied for user 'keystone'@'localhost' (using password: YES)")
  0.747 | scenario001/805/artifacts/artifacts/weirdo-project/logs/neutron/l3-agent.txt.gz:4953:    2017-06-20 09:35:18.750 30696 ERROR neutron.agent.linux.ip_lib ProcessExecutionError: Exit code: 2; Stdin: ; Stdout: ; Stderr: arping: Device qr-eab5db5e-2b not available.
  0.880 | scenario001/805/artifacts/artifacts/weirdo-project/logs/neutron/server.txt.gz:7395:      2017-06-20 09:24:16.539 1290 DEBUG oslo_db.api [req-5a32c588-c96d-43a5-a3c0-207232c3f399 75837f1fbb1645deb29271c270bfe910 37e84afc107a43f6bc40a74e35c294b2 - default default] Performing DB retry for function neutron.plugins.ml2.plugin.create_port: NeutronDbObjectDuplicateEntry: Failed to create a duplicate IpamAllocation: for attribute(s) ['PRIMARY'] with value(s) 10.100.0.2-8e029793-091b-4870-97a5-37e02c86a239 wrapper /usr/lib/python2.7/site-packages/oslo_db/api.py:152
  0.847 | scenario001/805/artifacts/artifacts/weirdo-project/logs/openvswitch/ovsdb-server.txt.gz:0022:    2017-06-20T09:33:06.479Z|00022|reconnect|ERR|tcp:127.0.0.1:34002: no response to inactivity probe after 6.32 seconds, disconnecting

  # -> Reduced 233185 log lines to 321

* Look for new events in log files:

.. code-block:: console

  $ logreduce --baseline /var/log/audit/audit.log.4 /var/log/audit/audit.log --context-length 0
  0.276 | /var/log/audit/audit.log:0606: type=USER_AUTH msg=audit(1498373150.931:1661763): pid=20252 uid=0 auid=1000 ses=19490 subj=unconfined_u:unconfined_r:unconfined_t:s0-s0:c0.c1023 msg='op=PAM:authentication grantors=pam_rootok acct="root" exe="/usr/bin/su" hostname=? addr=? terminal=pts/0 res=success'
  0.287 | /var/log/audit/audit.log:0607: type=USER_ACCT msg=audit(1498373150.931:1661764): pid=20252 uid=0 auid=1000 ses=19490 subj=unconfined_u:unconfined_r:unconfined_t:s0-s0:c0.c1023 msg='op=PAM:accounting grantors=pam_succeed_if acct="root" exe="/usr/bin/su" hostname=? addr=? terminal=pts/0 res=success'

  # Today the 'su' program was indeed used to recover a sudo bug...

* Re-using a model:

.. code-block:: console

  $ logreduce --baseline /var/log/audit/audit.log.4 --save ~/audit.model
  $ logreduce --load ~/audit.model /var/log/audit/audit.log


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
  sudo python3 setup.py develop

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

* Add gerrit support to target a review directly
* Add travis/github support to target a pull request directly
* Support automatic log analysis and reporting when a job failed,
  e.g. through jenkins publisher or zuul post jobs.
* Add tarball traversal in utils.files_iterator
* Improve tokenization tests
* Discard files that are 100% anomalous
* Run test in paralelle

Other ideas:

* Compare logreduce performance between two versions, perhaps using logreduce
  itself... logception!
* Find an alternative to lshf, the model currently spend 97% of the time in the
  lsh.kneighbors method...
* Investigate character n-gram instead of word vectorisation
* Investigate more advance model such as recurrent neural net, perhaps using
  tensorflow instead of scikit-learn
* Investigate learning failed logs to reduce common/useless failure expression


Contribute
----------

Contribution are most welcome, use **git-review** to propose a change.
Setup your ssh keys after sign in https://softwarefactory-project.io/auth/login
