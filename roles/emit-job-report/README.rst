Zuul return html report when present

If a job generates a file named report.html, then make the
job result points at the report instead of the logs directory.

This role is meant to be used after the upload-logs role to
re-use the zuul_log_path fact.
