0.8.4
=====

- Improve index name tokenizer by splitting '.' and removing hexadecimal components.
- Update default excludes list.

0.8.3
=====

- Better handle words without vowels to match hexadecimal without 'e' or 'a'.
- Add `--config` command line argument to specify fileset.

0.8.2
=====

- Handle zuul per-project lookup automatically.
- Improve tokenizer by removing words without vowels.
- Hidden files are now ignored.

0.8.1
=====

- Handle api url without a trailing slash.
- Improve tokenizer for key value separated by space.

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
