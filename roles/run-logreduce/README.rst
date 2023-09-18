An ansible role to run logreduce on the current build when it failed.

Add this to your post playbook, after the `fetch-output`, like so:

.. code-block:: yaml

   - hosts: localhost
     roles:
       - role: run-logreduce
         logreduce_zuul_web: https://softwarefactory-project.io/zuul/
         logreduce_model_store_url: https://softwarefactory-project.io/logs/classifiers

Run logreduce for your job by adding the following vars:

.. code-block:: yaml

   - job:
       name: my-job
       vars:
         logreduce_optin: true

**Role Variables**

.. zuul:rolevar:: logreduce_optin
   :default: false

   Set this to true to activate logreduce.

.. zuul:rolevar:: logreduce_zuul_web

   The zuul-web URL, to lookup baselines.

.. zuul:rolevar:: logreduce_model_store_url

   The URL where pre-built model can be found.
