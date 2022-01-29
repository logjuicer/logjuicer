// Copyright (C) 2022 Red Hat
// SPDX-License-Identifier: Apache-2.0

use pyo3::prelude::*;
use lazy_static::lazy_static;
use regex::Regex;

#[pyfunction]
fn process(line: &str) -> String {
    lazy_static! {
        static ref MONTH_RE: Regex = Regex::new("january|february|march|april|may|june|july|august|september|october|november|december").unwrap();
        static ref HTTP_RE: Regex = Regex::new("http[^ ]*").unwrap();
    }
    let line = line.to_lowercase();
    let line = HTTP_RE.replace_all(&line, "URL");
    let line = MONTH_RE.replace_all(&line, "MONTH");
    line.to_string()
}

#[pymodule]
fn logreduce_tokenizer(_py: Python, m: &PyModule) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(process, m)?)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tokenizer() {
        assert_eq!(process("Hello"), "hello");
        assert_eq!(process("http://test"), "URL");
    }
}
