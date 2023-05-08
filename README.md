# logreduce-rust

This repository contains the last version of the [logreduce][logreduce]'s project.

## Install

Install the `logreduce-cli` command line by running:

```
cargo install --git https://github.com/logreduce/logreduce-rust logreduce-cli
```

> If you don't have `cargo`, see this [install rust](https://www.rust-lang.org/tools/install) documentation.


## Use

Analyze a local file:

```ShellSession
$ logreduce-cli file /var/log/zuul/scheduler.log
```

Analyze a remote url:

```ShellSession
$ logreduce-cli url https://zuul/build/uuid
```

Save and re-use trained model using the `--model file-path` argument.


## Learn

To read more about the project:

- Initial presentation [blog post](https://opensource.com/article/18/9/quiet-log-noise-python-and-machine-learning)
- The command line specification: [./doc/adr/0001-architecture-cli.md](./doc/adr/0001-architecture-cli.md)
- How the tokenizer works: [Improving logreduce tokenizer](https://www.softwarefactory-project.io/improving-logreduce-with-rust.html)
- How the nearest neighbor works: [Implementing logreduce nearest neighbors](https://www.softwarefactory-project.io/implementing-logreduce-nearest-neighbors-model-in-rust.html)
- How the log file iterator works: [Introducing the BytesLines iterator](https://www.softwarefactory-project.io/introducing-the-byteslines-iterator.html)
- [Completing the first release of logreduce-rust](https://www.softwarefactory-project.io/completing-the-first-release-of-logreduce-rust.html)


## Contribute

Clone the project and run tests:

```
git clone https://github.comm/logreduce/logreduce-rust && cd logreduce-rust
cargo test && cargo fmt && cargo clippy
```

Run the project:

```
cargo run -p logreduce-cli -- --help
```

## Roadmap

* HTML report
* `current-build` target
* detect `prow` and `jenkins` url
* Reports minification


[logreduce]: https://github.com/logreduce/logreduce
