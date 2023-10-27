// Copyright (C) 2022 Red Hat
// SPDX-License-Identifier: Apache-2.0

#![warn(missing_docs)]

//! This library provides python bindings for the [logjuicer](https://github.com/logjuicer/logjuicer) project.

use logjuicer_index::F;
use pyo3::prelude::*;
use pyo3::types::PyCapsule;
use std::ffi::CString;

/// Tokenize a line
#[pyfunction]
fn process(line: &str) -> String {
    logjuicer_tokenizer::process(line)
}

/// Generate random log lines
#[pyfunction]
fn generate(size: usize) -> String {
    logjuicer_generate::gen_lines()
        .take(size)
        .collect::<Vec<String>>()
        .join("\n")
}

/// The python module
#[pymodule]
fn logjuicer_rust(_py: Python, m: &PyModule) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(process, m)?)?;
    m.add_function(wrap_pyfunction!(generate, m)?)?;

    /// Return an opaque Capsule with the model
    #[pyfn(m)]
    fn index(py: Python<'_>, baselines: Vec<String>) -> Result<&PyCapsule, PyErr> {
        let name = CString::new("model").unwrap();
        let model = logjuicer_index::index(&mut baselines.into_iter());
        PyCapsule::new(py, model, &name)
    }

    #[pyfn(m)]
    fn search(py: Python<'_>, pymodel: Py<PyCapsule>, target: String) -> F {
        let model = unsafe {
            pymodel
                .as_ref(py)
                .reference::<Vec<logjuicer_index::Features>>()
        };
        logjuicer_index::search(model, &target)
    }

    /// Return an opaque Capsule with the model
    #[pyfn(m)]
    fn index_mat(py: Python<'_>, baselines: Vec<String>) -> Result<&PyCapsule, PyErr> {
        let name = CString::new("model").unwrap();
        let model = logjuicer_index::index_mat(&baselines);
        PyCapsule::new(py, model, &name)
    }

    #[pyfn(m)]
    fn search_mat(py: Python<'_>, pymodel: Py<PyCapsule>, targets: Vec<String>) -> Vec<F> {
        let model = unsafe {
            pymodel
                .as_ref(py)
                .reference::<logjuicer_index::FeaturesMatrix>()
        };
        logjuicer_index::search_mat(model, &targets)
    }

    #[pyfn(m)]
    fn save_mat(py: Python<'_>, pymodel: Py<PyCapsule>) -> Vec<u8> {
        let model = unsafe {
            pymodel
                .as_ref(py)
                .reference::<logjuicer_index::FeaturesMatrix>()
        };
        logjuicer_index::save_mat(model)
    }

    #[pyfn(m)]
    fn load_mat(py: Python<'_>, buf: Vec<u8>) -> Result<&PyCapsule, PyErr> {
        let name = CString::new("model").unwrap();
        let model = logjuicer_index::load_mat(&buf);
        PyCapsule::new(py, model, &name)
    }

    Ok(())
}
