Upload classifiers model to a static webserver

Add this to your post playbook, before the `upload-logs`, like so:

.. code-block:: yaml

   - hosts: logserver
     roles:
       - role: upload-logreduce-model
       - role: upload-logs

**Role Variables**

.. zuul:rolevar:: logreduce_model_root
   :default: /var/www/logs/classifiers

   The location of the logreduce_model_store_url.
