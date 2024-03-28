// Copyright (C) 2023 Red Hat
// SPDX-License-Identifier: Apache-2.0

//! This module contains the database logic.

use sqlx::types::chrono::Utc;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use tokio::sync::Semaphore;

// provides `try_next`
use futures::TryStreamExt;

use logjuicer_model::{config::DiskSizeLimit, MODEL_VERSION};
use logjuicer_report::{
    model_row::{ContentID, ModelRow},
    report_row::{FileSize, ReportID, ReportRow, ReportStatus},
};

#[derive(Clone)]
pub struct Db {
    pool: sqlx::SqlitePool,
    sizes: Arc<AtomicUsize>,
}

const MODEL_VER: i64 = MODEL_VERSION as i64;
static JANITOR: Semaphore = Semaphore::const_new(1);

impl Db {
    pub async fn new(storage_dir: &str, sizes: Arc<AtomicUsize>) -> sqlx::Result<Db> {
        let db_url = format!("sqlite://{storage_dir}/logjuicer.sqlite?mode=rwc");
        let pool = sqlx::SqlitePool::connect(&db_url).await?;
        sqlx::migrate!("./migrations").run(&pool).await?;
        let db = Db { pool, sizes };
        db.clean_pending().await?;
        db.clean_old_models(storage_dir).await?;
        db.add_sizes(db.get_files_size().await?);
        Ok(db)
    }

    fn add_sizes(&self, value: usize) {
        self.sizes.fetch_add(value, Ordering::Relaxed);
    }

    fn sub_sizes(&self, value: usize) {
        let _ = self
            .sizes
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |prev| {
                Some(prev.saturating_sub(value))
            });
    }

    async fn get_files_size(&self) -> sqlx::Result<usize> {
        let models = sqlx::query!("select SUM(bytes_size) as size from models")
            .map(|r| r.size.unwrap_or(0))
            .fetch_one(&self.pool)
            .await?;
        let reports = sqlx::query!("select SUM(bytes_size) as size from reports")
            .map(|r| r.size.unwrap_or(0))
            .fetch_one(&self.pool)
            .await?;
        Ok(models as usize + reports as usize)
    }

    async fn clean_pending(&self) -> sqlx::Result<()> {
        let status = ReportStatus::Pending.as_str();
        sqlx::query!("delete from reports where status = ?", status)
            .execute(&self.pool)
            .await
            .map(|_| ())
    }

    #[cfg(test)]
    pub async fn deprecate_models(&self) -> sqlx::Result<()> {
        sqlx::query!("update models set version = 0")
            .execute(&self.pool)
            .await
            .map(|_| ())
    }

    #[cfg(test)]
    pub async fn increase_report_age(&self) -> sqlx::Result<()> {
        sqlx::query!("update reports set created_at = '2024-01-01'")
            .execute(&self.pool)
            .await
            .map(|_| ())
    }

    pub async fn reclaim_space(
        &self,
        storage_dir: &str,
        disk_size_limit: DiskSizeLimit,
    ) -> sqlx::Result<Option<(usize, usize, usize)>> {
        let _janitor = JANITOR.acquire().await.unwrap();
        let current = self.sizes.load(Ordering::Relaxed);
        if current > disk_size_limit.max {
            let amount = current - disk_size_limit.min;
            match self.do_reclaim_space(storage_dir, amount).await {
                Ok((model_count, report_count)) => {
                    tracing::info!(amount, model_count, report_count, "Reclaimed disk space");
                    Ok(Some((amount, model_count, report_count)))
                }
                Err(e) => {
                    tracing::error!(error = e.to_string(), "Could not reclaim disk space");
                    Err(e)
                }
            }
        } else {
            Ok(None)
        }
    }

    async fn do_reclaim_space(
        &self,
        storage_dir: &str,
        mut amount: usize,
    ) -> sqlx::Result<(usize, usize)> {
        let last_week = Utc::now().checked_sub_days(chrono::Days::new(7)).unwrap();
        let last_week_str = format!("{}", last_week.format("%Y-%m-%d"));
        let mut model_count: usize = 0;
        let mut report_count: usize = 0;
        while amount > 0 {
            // remove models
            let models = sqlx::query!(
                "SELECT content_id, bytes_size FROM models WHERE created_at < ? ORDER BY created_at ASC LIMIT 20",
                last_week_str
            )
            .fetch_all(&self.pool)
            .await?;
            let has_models = !models.is_empty();
            for row in models {
                crate::models::delete_model(storage_dir, &(row.content_id.clone().into()));
                let model_size = row.bytes_size.unwrap_or(0) as usize;
                self.sub_sizes(model_size);
                amount = amount.saturating_sub(model_size);
                model_count += 1;
                sqlx::query!("delete from models where content_id != ?", row.content_id)
                    .execute(&self.pool)
                    .await
                    .map(|_| ())?;
                if amount == 0 {
                    break;
                }
            }
            if amount == 0 {
                break;
            }

            // remove reports
            let reports = sqlx::query!(
                "SELECT id, bytes_size FROM reports WHERE created_at < ? ORDER BY created_at ASC LIMIT 20",
                last_week_str
            )
            .fetch_all(&self.pool)
            .await?;
            let has_reports = !reports.is_empty();
            for row in reports {
                let report_size = row.bytes_size.unwrap_or(0) as usize;
                self.sub_sizes(report_size);
                amount = amount.saturating_sub(report_size);
                report_count += 1;
                let _ = std::fs::remove_file(report_path(storage_dir, ReportID(row.id)));
                sqlx::query!("delete from reports where id != ?", row.id)
                    .execute(&self.pool)
                    .await
                    .map(|_| ())?;
                if amount == 0 {
                    break;
                }
            }

            if !has_reports && !has_models {
                break;
            }
        }
        Ok((model_count, report_count))
    }

    async fn clean_old_models(&self, storage_dir: &str) -> sqlx::Result<()> {
        let mut rows = sqlx::query!(
            "select content_id from models where version != ?",
            MODEL_VER
        )
        .map(|row| row.content_id.into())
        .fetch(&self.pool);
        let mut clean_count = 0;
        while let Some(content_id) = rows.try_next().await? {
            crate::models::delete_model(storage_dir, &content_id);
            clean_count += 1;
        }
        if clean_count > 0 {
            tracing::info!(count = clean_count, "Cleaned old models");
        }
        sqlx::query!("delete from models where version != ?", MODEL_VER)
            .execute(&self.pool)
            .await
            .map(|_| ())
    }

    pub async fn get_reports(&self) -> sqlx::Result<Vec<ReportRow>> {
        sqlx::query_as!(
        ReportRow,
        "select id, created_at, updated_at, target, baseline, anomaly_count, status, bytes_size from reports order by id desc"
    )
        .fetch_all(&self.pool)
        .await
    }

    pub async fn get_report_status(
        &self,
        report_id: ReportID,
    ) -> sqlx::Result<Option<ReportStatus>> {
        sqlx::query!("select status from reports where id = ?", report_id.0)
            .map(|row| row.status.into())
            .fetch_optional(&self.pool)
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
        .fetch_optional(&self.pool)
        .await
    }

    pub async fn update_report(
        &self,
        report_id: ReportID,
        anomaly_count: usize,
        status: &ReportStatus,
        size: FileSize,
    ) -> sqlx::Result<()> {
        let now = Utc::now();
        let count = anomaly_count as i64;
        self.add_sizes(size.0 as usize);
        let size = size.0 as i64;
        let status = status.as_str();
        sqlx::query!(
            "update reports set updated_at = ?, anomaly_count = ?, status = ?, bytes_size = ? where id = ?",
            now,
            count,
            status,
            size,
            report_id.0,
        )
        .execute(&self.pool)
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
        .execute(&self.pool)
        .await?
        .last_insert_rowid();
        Ok(id.into())
    }

    pub async fn get_models(&self) -> sqlx::Result<Vec<ModelRow>> {
        sqlx::query_as!(
            ModelRow,
            "select content_id, version, created_at, bytes_size from models"
        )
        .fetch_all(&self.pool)
        .await
    }

    pub async fn lookup_model(&self, content_id: &ContentID) -> sqlx::Result<Option<()>> {
        sqlx::query!(
            "select version from models where content_id = ?",
            content_id.0
        )
        .map(|_row| ())
        .fetch_optional(&self.pool)
        .await
    }

    pub async fn add_model(&self, content_id: &ContentID, size: FileSize) -> sqlx::Result<()> {
        let now_utc = Utc::now();
        self.add_sizes(size.0 as usize);
        let size = size.0 as i64;
        sqlx::query!(
            "insert into models (content_id, version, created_at, bytes_size)
                      values (?, ?, ?, ?)",
            content_id.0,
            MODEL_VER,
            now_utc,
            size,
        )
        .execute(&self.pool)
        .await
        .map(|_| ())
    }
}

pub fn report_path(storage_dir: &str, rid: ReportID) -> String {
    format!("{}/{}.gz", storage_dir, rid)
}
