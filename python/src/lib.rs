// Copyright (C) 2022 Red Hat
// SPDX-License-Identifier: Apache-2.0

#![warn(missing_docs)]

//! This library provides a python binding for the [logreduce](https://github.com/logreduce/logreduce) project.

use pyo3::prelude::*;

/// The python function
#[pyfunction]
fn process(line: &str) -> String {
    logreduce_tokenizer::tokenizer::process(line)
}

/// The python module
#[pymodule]
fn logreduce_python(_py: Python, m: &PyModule) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(process, m)?)?;

    Ok(())
}
