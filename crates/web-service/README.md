# logreduce-web-service

This crate provides the `logreduce-api` command to provide a web-service for running logreduce.


## Usage

```ShellSession
podman run --name logreduce --volume logreduce-data:/data --publish 3000:3000 logreduce
```

## API

The service is designed to be access with the [logreduce-web](../web) application.
But it can also be used with curl:


### List reports

```ShellSession
curl localhost:3030/api/reports | jq
```

Returns the following list of [report row](../report/src/report_row.rs):

```rust
pub enum ReportStatus {
    Pending,
    Completed,
    Error(String),
}

pub struct ReportRow {
    pub id: ReportID,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
    pub target: Box<str>,
    pub baseline: Box<str>,
    pub anomaly_count: i64,
    pub status: ReportStatus,
}
```

### Get a report

```ShellSession
curl localhost:3030/api/report/$REPORT_ID
```

Return the report containing the anomalies.

### Create a report

```ShellSession
curl localhost:3030/api/report/new?target=$URL
```

Returns the ReportID, ReportStatus

### Watch a report

```ShellSession
curl ws://localhost:3030/wsapi/report/$REPORT_ID
```

Watch the report creation process.


## Contribute

Hot reload the service with `cargo watch -x run`.

When changing migrations or sqlx macro usages, run: `cargo sqlx prepare`.

To create the database manually:

```ShellSession
sqlx database create
sqlx migrate run
```
