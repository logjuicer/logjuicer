// Copyright (C) 2022 Red Hat
// SPDX-License-Identifier: Apache-2.0

#![warn(missing_docs)]

//! This library provides python bindings for the [logreduce](https://github.com/logreduce/logreduce) project.

use pyo3::prelude::*;
use pyo3::types::PyCapsule;
use std::ffi::CString;
use logreduce_index::F;

/// Tokenize a line
#[pyfunction]
fn process(line: &str) -> String {
    logreduce_tokenizer::process(line)
}

/// Generate random log lines
#[pyfunction]
fn generate(size: usize) -> String {
    logreduce_generate::gen_lines()
        .take(size)
        .collect::<Vec<String>>()
        .join("\n")
}

/// The python module
#[pymodule]
fn logreduce_rust(_py: Python, m: &PyModule) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(process, m)?)?;
    m.add_function(wrap_pyfunction!(generate, m)?)?;

    /// Return an opaque Capsule with the model
    #[pyfn(m)]
    fn index(py: Python<'_>, baselines: Vec<String>) -> Result<&PyCapsule, PyErr> {
        let name = CString::new("model").unwrap();
        let model = logreduce_index::index(&mut baselines.into_iter());
        PyCapsule::new(py, model, &name)
    }

    #[pyfn(m)]
    fn search(py: Python<'_>, pymodel: Py<PyCapsule>, target: String) -> F {
        let model = unsafe {
            pymodel
                .as_ref(py)
                .reference::<Vec<logreduce_index::Features>>()
        };
        logreduce_index::search(model, &target)
    }

    /// Return an opaque Capsule with the model
    #[pyfn(m)]
    fn index_mat(py: Python<'_>, baselines: Vec<String>) -> Result<&PyCapsule, PyErr> {
        let name = CString::new("model").unwrap();
        let model = logreduce_index::index_mat(&mut baselines.into_iter());
        PyCapsule::new(py, model, &name)
    }

    #[pyfn(m)]
    fn search_mat(py: Python<'_>, pymodel: Py<PyCapsule>, targets: Vec<String>) -> Vec<F> {
        let model = unsafe {
            pymodel
                .as_ref(py)
                .reference::<logreduce_index::FeaturesMatrix>()
        };
        logreduce_index::search_mat(model, &mut targets.into_iter())
    }

    #[pyfn(m)]
    fn save_mat(py: Python<'_>, pymodel: Py<PyCapsule>) -> Vec<u8> {
        let model = unsafe {
            pymodel
                .as_ref(py)
                .reference::<logreduce_index::FeaturesMatrix>()
        };
        logreduce_index::save_mat(model)
    }

    #[pyfn(m)]
    fn load_mat(py: Python<'_>, buf: Vec<u8>) -> Result<&PyCapsule, PyErr> {
        let name = CString::new("model").unwrap();
        let model = logreduce_index::load_mat(&buf);
        PyCapsule::new(py, model, &name)
    }

    Ok(())
}
