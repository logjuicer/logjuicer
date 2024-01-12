# LogJuicer Extracts Anomalies From Log Files

Based on baseline logs, LogJuicer highlights useful texts in target logs.
The goal is to save time in finding failures' root causes.


## How it works

LogJuicer implements a custom diffing process to compare logs:

* A tokenizer removes random words.
* Lines are converted into feature vectors using the hashing trick.
* The logs are compared using cosine similarity.


## Install

Install the `logjuicer` command line by running:

```
$ cargo install --git https://github.com/logjuicer/logjuicer logjuicer-cli
```

> If you don't have `cargo`, see this [install rust](https://www.rust-lang.org/tools/install) documentation.

Or grab the latest release assets `logjuicer-x86_64-linux.tar.bz2` from <https://github.com/logjuicer/logjuicer/releases>


## Use

Analyze a local file:

```ShellSession
$ logjuicer file /var/log/zuul/scheduler.log
```

Analyze a remote url:

```ShellSession
$ logjuicer url https://zuul/build/uuid
```

Compare two inputs (when baseline discovery doesn't work):

```ShellSession
$ logjuicer diff https://zuul/build/success-build https://zuul/build/failed-build
```

Save and re-use trained model using the `--model file-path` argument.


## Configure

LogJuicer supports the [ant's fileset](https://ant.apache.org/manual/Types/fileset.html) configuration to
filter the processed files:

- *includes*: list of files regex that must be included. Defaults to all files.
- *excludes*: list of files regex that must be excluded. Defaults to default excludes or none if `default_excludes` is false.
- *default_excludes*: indicates whether [default excludes](./crates/model/src/config/default_excludes.rs) should be used or not.


## Learn

To read more about the project:

- Initial presentation [blog post](https://opensource.com/article/18/9/quiet-log-noise-python-and-machine-learning)
- The command line specification: [./doc/adr/0001-architecture-cli.md](./doc/adr/0001-architecture-cli.md)
- How the tokenizer works: [Improving LogJuicer Tokenizer](https://www.softwarefactory-project.io/improving-logreduce-with-rust.html)
- How the nearest neighbor works: [Implementing LogJuicer Nearest Neighbors](https://www.softwarefactory-project.io/implementing-logreduce-nearest-neighbors-model-in-rust.html)
- How the log file iterator works: [Introducing the BytesLines Iterator](https://www.softwarefactory-project.io/introducing-the-byteslines-iterator.html)
- [Completing the first release of LogJuicer](https://www.softwarefactory-project.io/completing-the-first-release-of-logreduce-rust.html)
- How the web interface works: [WASM based web interface](https://www.softwarefactory-project.io/logreduce-wasm-based-web-interface.html)
- The report file format: [Leveraging Cap'n Proto For LogJuicer Reports](https://www.softwarefactory-project.io/leveraging-capn-proto-for-logreduce-reports.html)


## Contribute

Clone the project and run tests:

```
git clone https://github.com/logjuicer/logjuicer && cd logjuicer
cargo test && cargo fmt && cargo clippy
```

Run the project:

```
cargo run -p logjuicer-cli -- --help
```

Activate tracing debug:

```
export LOGJUICER_LOG="logjuicer_model=debug,logjuicer_cli=debug"
# Create a chrome trace that can be viewed in web browser with `chrome://tracing`
export LOGJUICER_TRACE=./chrome.trace
```

Join the project Matrix room: [#logjuicer:matrix.org](https://matrix.to/#/#logjuicer:matrix.org).


## Roadmap

* Detect `jenkins` url
* Reports minification
* Web service deployment

[logjuicer]: https://github.com/logjuicer/logjuicer
