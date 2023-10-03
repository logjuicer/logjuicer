CREATE TABLE IF NOT EXISTS reports
(
    id          INTEGER PRIMARY KEY NOT NULL,
    created_at  TIMESTAMP           NOT NULL,
    updated_at  TIMESTAMP           NOT NULL,
    target      TEXT                NOT NULL,
    baseline    TEXT                NOT NULL,
    anomaly_count INTEGER           NOT NULL,
    status      TEXT                NOT NULL
);
