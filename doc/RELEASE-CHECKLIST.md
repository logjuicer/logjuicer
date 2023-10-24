Release Checklist
-----------------

These notes documents the release process for logreduce.

- When the tokenizer or the model struct changed, bump the *MODEL_VERSION* in `crates/model/src/model.rs`.
- Bump the version in `Cargo.toml` and run `cargo check` to update the lock file..
- Rename *next-version* and add a new template to the the `CHANGELOG.md`.
- Create and push a new signed tag.
- Wait for CI to finish creating the release.
- Copy the CHANGELOG to the release body.
- Update the *logreduce_version* in `roles/run-logreduce/defaults/main.yaml`
