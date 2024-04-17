// Copyright (C) 2024 Red Hat
// SPDX-License-Identifier: Apache-2.0

//! This module contains the database data type shared between the api and the web client.

use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};

use crate::Content;

#[derive(Clone, Debug, PartialOrd, Ord, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ContentID(pub Box<str>);

impl std::str::FromStr for ContentID {
    type Err = ();

    fn from_str(src: &str) -> Result<ContentID, ()> {
        Ok(ContentID(src.into()))
    }
}

impl From<String> for ContentID {
    #[inline]
    fn from(src: String) -> ContentID {
        ContentID(src.into())
    }
}

impl From<&Content> for ContentID {
    #[inline]
    fn from(src: &Content) -> ContentID {
        ContentID(src.to_string().into())
    }
}

impl ContentID {
    pub fn as_str(&self) -> &str {
        self.0.as_ref()
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ModelRow {
    pub content_id: ContentID,
    pub version: i64,
    pub created_at: NaiveDateTime,
}
