# logjuicer-web-service

This crate provides the `logjuicer-api` command to provide a web-service for running logjuicer.


## Usage

```ShellSession
podman run --name logjuicer --volume logjuicer-data:/data --publish 3000:3000 logjuicer
```

## API

The service is designed to be access with the [logjuicer-web](../web) application.
But it can also be used with curl:


### List reports

```ShellSession
curl localhost:3000/api/reports | jq
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
curl localhost:3000/api/report/$REPORT_ID
```

Return the report containing the anomalies.

### Create a report

```ShellSession
curl localhost:3000/api/report/new?target=$URL
```

Returns the ReportID, ReportStatus

### Watch a report

```ShellSession
curl ws://localhost:3000/wsapi/report/$REPORT_ID
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
