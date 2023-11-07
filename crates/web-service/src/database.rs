// Copyright (C) 2023 Red Hat
// SPDX-License-Identifier: Apache-2.0

//! This module contains the database logic.

use sqlx::types::chrono::Utc;

use logjuicer_report::report_row::{ReportID, ReportRow, ReportStatus};

#[derive(Clone)]
pub struct Db(sqlx::SqlitePool);

impl Db {
    pub async fn new() -> sqlx::Result<Db> {
        let db_url = "sqlite://data/logjuicer.sqlite?mode=rwc";
        let pool = sqlx::SqlitePool::connect(db_url).await?;
        sqlx::migrate!("./migrations").run(&pool).await?;
        let db = Db(pool);
        db.clean_pending().await?;
        Ok(db)
    }

    async fn clean_pending(&self) -> sqlx::Result<()> {
        let status = ReportStatus::Pending.as_str();
        sqlx::query!("delete from reports where status = ?", status)
            .execute(&self.0)
            .await
            .map(|_| ())
    }

    pub async fn get_reports(&self) -> sqlx::Result<Vec<ReportRow>> {
        sqlx::query_as!(
        ReportRow,
        "select id, created_at, updated_at, target, baseline, anomaly_count, status from reports order by id desc"
    )
        .fetch_all(&self.0)
        .await
    }

    pub async fn get_report_status(
        &self,
        report_id: ReportID,
    ) -> sqlx::Result<Option<ReportStatus>> {
        sqlx::query!("select status from reports where id = ?", report_id.0)
            .map(|row| row.status.into())
            .fetch_optional(&self.0)
            .await
    }

    pub async fn lookup_report(
        &self,
        target: &str,
        baseline: &str,
    ) -> sqlx::Result<Option<(ReportID, ReportStatus)>> {
        sqlx::query!(
            "select id, status from reports where target = ? and baseline = ?",
            target,
            baseline
        )
        .map(|row| (row.id.into(), row.status.into()))
        .fetch_optional(&self.0)
        .await
    }

    pub async fn update_report(
        &self,
        report_id: ReportID,
        anomaly_count: usize,
        status: &ReportStatus,
    ) -> sqlx::Result<()> {
        let now = Utc::now();
        let count = anomaly_count as i64;
        let status = status.as_str();
        sqlx::query!(
            "update reports set updated_at = ?, anomaly_count = ?, status = ? where id = ?",
            now,
            count,
            status,
            report_id.0
        )
        .execute(&self.0)
        .await
        .map(|_| ())
    }

    pub async fn initialize_report(&self, target: &str, baseline: &str) -> sqlx::Result<ReportID> {
        let now_utc = Utc::now();
        let status = ReportStatus::Pending.as_str();
        let id = sqlx::query!(
            "insert into reports (created_at, updated_at, target, baseline, anomaly_count, status)
                      values (?, ?, ?, ?, ?, ?)",
            now_utc,
            now_utc,
            target,
            baseline,
            0,
            status
        )
        .execute(&self.0)
        .await?
        .last_insert_rowid();
        Ok(id.into())
    }
}
