logreduce - extract anomaly from log files
==========================================

Based on success logs, logreduce highlights useful text in failed logs.
The goal is to assist in debugging failure and reduce effort needed to read
boring log files.

On average, learning run at 8k lines per second, and
testing run at 0.400k lines per seconds.


How it works
------------

logreduce uses a *model* to learn successful logs and detect outliers in
failed log that doesn't fit the model. The model is constructed as follow:

* Random words are manually removed using regular expression
* Striped lines are converted into a dictionary based numeric vector
  (using **CountVectorizer**),
* Vector are weighted based on term frequencies times inverse
  document-frequency (using **TfidfTransformer**),
* Then vector are used in a unsupervised nearest neighbor learning model.

There are currently two model:

* simple, using **NearestNeighbors**
* lsfh, using **LSHForest**

In short, logreduce relies heavily on line stripping with a bag-of-words
technique and it uses the distance to known sentences to detect outliers.

For example this input:

.. code-block:: console

  2017-06-21 04:37:45,827 INFO [nodepool.builder.UploadWorker.0] Uploading DIB image build 0000000002 from /tmpxvLOTg/fake-image-0000000002.qcow2 to fake-provider

Results in:

.. code-block:: console

  INFO nodepool builder UploadWorker Uploading image build from /fake image fake provider


The tokenization makes the model a bit dependent on the target data, for example,
to support OpenStack logs, words begining by ns- or req- are taken into account.
Further improvement such as characters n-gram may remove such limitation.


Caveats
-------

This method doesn't work when debug content is only included in failed logs.
To successfully detect anomalies, failed and success logs needs to be similar,
otherwise all the extra information in failed logs will be considered anomalous.

This is currently the case for tripleo ovb ci where overcloud logs are
only included in the failed logs, resulting in a lot of false-positive.

This also happens with testr tests where success logs only contains 'SUCCESS'.


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

* Fetch bootstrap for nicer html output

.. code-block:: console

  curl -O https://maxcdn.bootstrapcdn.com/bootstrap/3.3.7/css/bootstrap.min.css
  curl -O https://maxcdn.bootstrapcdn.com/bootstrap/3.3.7/js/bootstrap.min.js

Usage
-----

Log files can be a single file or a directory.

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

The model can be trained and saved for re-use using **--save**.
When using **--load** logreduce doesn't need a **--baseline**.

Full usage:

.. code-block:: console

  $ usage: logreduce [-h] [--debug] [--debug-token] [--update-cache]
                   [--ignore-file IGNORE_FILE [IGNORE_FILE ...]]
                   [--model {simple,lshf,noop}]
                   [--output-format {text,json,yaml,pprint,html}] [--save FILE]
                   [--load FILE] [--jenkins-url JENKINS_URL] [--fetch-artifacts]
                   [--threshold THRESHOLD] [--merge-distance MERGE_DISTANCE]
                   [--before-context BEFORE_CONTEXT]
                   [--after-context AFTER_CONTEXT] [--baseline LOG]
                   [target [target ...]]

  positional arguments:
    target                Failed logs

  optional arguments:
    -h, --help            show this help message and exit
    --debug               Print debug
    --debug-token         Print tokenization process
    --ignore-file IGNORE_FILE [IGNORE_FILE ...]
    --model {simple,lshf,noop}
    --output-format {text,json,yaml,pprint,html}
    --save FILE           Save the model
    --load FILE           Load a previous model
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
    --baseline LOG        Success logs


See bellow for some examples


Examples
--------

* Look for new events in log files:

.. code-block:: console

  $ logreduce --baseline /var/log/audit/audit.log.4 /var/log/audit/audit.log --context-length 0
  0.276 | /var/log/audit/audit.log:0606: type=USER_AUTH msg=audit(1498373150.931:1661763): pid=20252 uid=0 auid=1000 ses=19490 subj=unconfined_u:unconfined_r:unconfined_t:s0-s0:c0.c1023 msg='op=PAM:authentication grantors=pam_rootok acct="root" exe="/usr/bin/su" hostname=? addr=? terminal=pts/0 res=success'
  0.287 | /var/log/audit/audit.log:0607: type=USER_ACCT msg=audit(1498373150.931:1661764): pid=20252 uid=0 auid=1000 ses=19490 subj=unconfined_u:unconfined_r:unconfined_t:s0-s0:c0.c1023 msg='op=PAM:accounting grantors=pam_succeed_if acct="root" exe="/usr/bin/su" hostname=? addr=? terminal=pts/0 res=success'

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
