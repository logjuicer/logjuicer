Upload classifiers model to a static webserver

Add this to your post playbook, before the `upload-logs`, like so:

.. code-block:: yaml

   - hosts: logserver
     roles:
       - role: upload-logjuicer-model
       - role: upload-logs

**Role Variables**

.. zuul:rolevar:: logjuicer_model_root

   The location of the logjuicer_model_store_url.
