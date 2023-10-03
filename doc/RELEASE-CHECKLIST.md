Release Checklist
-----------------

These notes documents the release process for logreduce.

# logreduce

- When the report struct changes, make a new logreduce-web release.
- When the tokenizer changes, update the *MODEL_VERSION* in `crates/model/src/model.rs`.
- Bump the version in `crates/cli/Cargo.toml`.
- Rename *next-version* and add a new template to the the `CHANGELOG.md`.
- Create and push a new signed tag.
- Wait for CI to finish creating the release.
- Copy the CHANGELOG to the release body.
- Update the *logreduce_version* in `roles/run-logreduce/defaults/main.yaml`

# logreduce-web

- Bump the version in `crates/web/Cargo.toml`.
- Run the `publish-logreduce-web` workflow action.
- Update the *logreduce_web_version* in `roles/run-logreduce/defaults/main.yaml`
