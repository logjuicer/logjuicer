# logjuicer-web

This crate provides the `logjuicer-web` application to render report and access the [logjuicer-api](../web-service/).

## Usage

The application support the following SPA URL:

### Create a report: `/report/new?target=$URL`

### Compare two urls: `/report/new?target=$URL&baseline=$URL`

### Watch a report `/report/watch/$REPORT_ID`

### Read report: `/report/$REPORT_ID`


## Contribute

The application comes in two flavors:

- report: the report viewer (when build with `--no-default-features`)
- api_client: the full web application for the web service (this is the default).

To build the web interface, you need to use [trunk](https://trunkrs.dev/). Get it by running:

```
cargo install --locked trunk
```

### Report viewer

To work on the report viewer, you first need to generate a report named `logjuicer.bin` using the following command:

```ShellSession
cargo run -p logjuicer-cli -- --report logjuicer.bin diff|url ...
```

If you are in a hurry, you can simply run `diff Cargo.toml Trunk.toml`.

Then you can build, serve and hot-reload the report viewer by running the following trunk command:

```ShellSession
trunk serve ./dev.html --address 0.0.0.0 --no-default-features
```

### API Client

To work on the api client, you first need to start the API. Checkout the [web-service crate](../web-service).

Then you can build, serve and hot-reload the api client by running the following trunk command:

```ShellSession
trunk serve ./index.html --address 0.0.0.0
```
