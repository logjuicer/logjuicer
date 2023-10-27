# LogJuicer zuul-jobs roles

To integrate logjuicer into your [zuul-ci](https://zuul-ci.org),
copy the roles to your config repository and update the post playbook
like this:

```yaml
- name: collect logs
  hosts: localhost
  roles:
    - role: generate-zuul-manifest
    - role: run-logjuicer
      logjuicer_zuul_web: https://zuul.sftests.com/
      logjuicer_model_store_url: https://logserver.sftests.com/logs/classifiers
    - role: add-fileserver
      fileserver: "{{ site_sflogs }}"

- name: upload logs
  hosts: logserver-sshd
  roles:
    - role: upload-logjuicer-model
      logjuicer_model_root: rsync/classifiers
    - role: upload-logs
      zuul_log_compress: true
      zuul_log_url: "https://logserver.sftests.com/logs/"
      zuul_logserver_root: "{{ site_sflogs.path }}"
      zuul_log_verbose: true
```
