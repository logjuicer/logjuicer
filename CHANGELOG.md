0.8.1
=====

- Handle api url without a trailing slash.

0.8.0
=====

- Prow build url can now be used to extract anomalies from prow build artifacts.
- Log files ending with `.log.1.gz` and `.log.txt.gz` are now handled correctly.
- Reduce logreduce command binary size by replacing the reqwest http client with ureq.

0.7.1
=====

- Improve index name tokenization to handle kubernetes random string.
- Add statistics to the command line output.
- New debug-model command.
- Fix bad release tarball permissions.

0.7.0
=====

Initial Rust based release.


0.6.2
=====

Last Python based release.
