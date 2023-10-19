next-version
============

0.9.0
=====

- release: Merge the web, api and cli release into a single version.
- cli: Generate the html file along with the binary report.
- httpdir: Ignore "index of" page footer in httpdir query to avoid 404 errors.
- model: Do not skip zuul baseline build when they have no ref or change associated.
- web: Make the log report header sticky.
- report: Serialize using capnproto to implement backward compatibility.

0.8.5
=====

- Add samples_count query to check how large is an index.
- Fix http dir query not using the provided certificate.
- Try to use the default /etc/pki/tls/certs/ca-bundle.crt CA for TLS verification.
- Support targro http directory.
- web: Fix url of invalid report.
- web: Add log line anchors.

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
