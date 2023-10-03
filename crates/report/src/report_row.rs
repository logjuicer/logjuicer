// Copyright (C) 2023 Red Hat
// SPDX-License-Identifier: Apache-2.0

//! This module contains the database data type shared between the api and the web client.

use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};

#[derive(Copy, Clone, Debug, PartialOrd, Ord, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ReportID(pub i64);

impl std::str::FromStr for ReportID {
    type Err = std::num::ParseIntError;

    fn from_str(src: &str) -> Result<ReportID, std::num::ParseIntError> {
        i64::from_str(src).map(ReportID)
    }
}

impl std::fmt::Display for ReportID {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<i64> for ReportID {
    #[inline]
    fn from(i: i64) -> ReportID {
        ReportID(i)
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub enum ReportStatus {
    Pending,
    Completed,
    Error(String),
}

impl ReportStatus {
    pub fn as_str(&self) -> &str {
        match self {
            ReportStatus::Pending => "pending",
            ReportStatus::Completed => "done",
            ReportStatus::Error(e) => e.as_str(),
        }
    }
}

impl From<String> for ReportStatus {
    fn from(value: String) -> Self {
        match value.as_str() {
            "pending" => ReportStatus::Pending,
            "done" => ReportStatus::Completed,
            _ => ReportStatus::Error(value),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ReportRow {
    pub id: ReportID,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
    pub target: Box<str>,
    pub baseline: Box<str>,
    pub anomaly_count: i64,
    pub status: ReportStatus,
}
