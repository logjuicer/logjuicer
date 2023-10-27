An ansible role to run logjuicer on the current build when it failed.

Add this to your post playbook, after the `fetch-output`, like so:

.. code-block:: yaml

   - hosts: localhost
     roles:
       - role: run-logjuicer
         logjuicer_zuul_web: https://softwarefactory-project.io/zuul/
         logjuicer_model_store_url: https://softwarefactory-project.io/logs/classifiers

Run logjuicer for your job by adding the following vars:

.. code-block:: yaml

   - job:
       name: my-job
       vars:
         logjuicer_optin: true

**Role Variables**

.. zuul:rolevar:: logjuicer_optin
   :default: false

   Set this to true to activate logjuicer.

.. zuul:rolevar:: logjuicer_zuul_web

   The zuul-web URL, to lookup baselines.

.. zuul:rolevar:: logjuicer_model_store_url

   The URL where pre-built model can be found.
