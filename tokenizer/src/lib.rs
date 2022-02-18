// Copyright (C) 2022 Red Hat
// SPDX-License-Identifier: Apache-2.0

#![warn(missing_docs)]

//! This library provides a tokenizer function for the [logreduce](https://github.com/logreduce/logreduce) project.
//!
//! The goal is to replace varying words with fixed tokens (e.g. `sha256://...` is converted to `%HASH`).
pub mod tokenizer;
