// Copyright (C) 2022 Red Hat
// SPDX-License-Identifier: Apache-2.0

#![warn(missing_docs)]

//! This library provides a tokenizer function for the [logreduce](https://github.com/logreduce/logreduce) project.
//!
//! The goal is to replace varying words with fixed tokens (e.g. `sha256://...` is converted to `%HASH`).

use pyo3::prelude::*;

pub mod tokenizer;

/// The python function
#[pyfunction]
fn process(line: &str) -> String {
    tokenizer::process(line)
}

/// The python module
#[pymodule]
fn logreduce_tokenizer(_py: Python, m: &PyModule) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(process, m)?)?;

    Ok(())
}
