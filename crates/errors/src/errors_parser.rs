// Copyright (C) 2025 Red Hat
// SPDX-License-Identifier: Apache-2.0

#![warn(missing_docs)]
#![allow(clippy::manual_map)]

//! This library provides an error parsing function for the [logjuicer](https://github.com/logjuicer/logjuicer) project.
//!
//! The goal is to detect if a line contains an error or if it is part of a multiline stack trace.

use lazy_static::lazy_static;
use regex::Regex;

/// The parser state
pub struct State(Parser);
enum Parser {
    /// Not parsing anything
    Unknown,
    /// Parsing a Python Traceback
    PythonTraceback(usize),
    /// Parsing a Golang Stacktrace
    GoStacktrace(usize, GoStatus),
}

enum GoStatus {
    /// Reading the header line
    Header,
    /// Reading the first goroutine
    Routine,
    /// Reading the goroutine stacks
    Threads,
}

/// The result of parsing a line.
pub enum Result {
    /// The line doesn't containt an error
    NoError,
    /// The line is a standalone error
    Error,
    /// More lines is needed to complete the error
    NeedMore,
    /// The line completes a multi-line traceback
    CompletedTraceBack,
}

impl State {
    /// Initialize the parser.
    pub fn new() -> State {
        State(Parser::Unknown)
    }
    /// Parse a single line.
    pub fn parse(&mut self, line: &str) -> Result {
        match self.0 {
            Parser::Unknown => {
                if let Some(state) = is_multiline(line) {
                    self.need_more(state)
                } else if is_error_line(line) {
                    Result::Error
                } else {
                    Result::NoError
                }
            }
            Parser::PythonTraceback(pos) => match line.chars().nth(pos) {
                // Python traceback continues until first character is not a space.
                None | Some(' ') => Result::NeedMore,
                _ => self.complete(Result::CompletedTraceBack),
            },
            Parser::GoStacktrace(pos, GoStatus::Header) => match line.chars().nth(pos) {
                // Go traceback can begin with a signal debug statement.
                Some('[') => Result::NeedMore,
                // Go traceback are separated by an empty line.
                None => self.need_more(Parser::GoStacktrace(pos, GoStatus::Routine)),
                // The previous 'panic:' was not valid
                _ => self.complete(Result::NoError),
            },
            Parser::GoStacktrace(pos, GoStatus::Routine) => {
                if line.len() > pos && line[pos..].starts_with("goroutine ") {
                    self.need_more(Parser::GoStacktrace(pos, GoStatus::Threads))
                } else {
                    self.complete(Result::NoError)
                }
            }
            Parser::GoStacktrace(pos, GoStatus::Threads) => {
                if go_tb_completed(pos, line) {
                    self.complete(Result::CompletedTraceBack)
                } else {
                    Result::NeedMore
                }
            }
        }
    }
    fn complete(&mut self, result: Result) -> Result {
        self.0 = Parser::Unknown;
        result
    }
    fn need_more(&mut self, state: Parser) -> Result {
        self.0 = state;
        Result::NeedMore
    }
}

/// Check if the line starts with the neddle, or if it is preceded by a space.
fn start_find(line: &str, needle: &str) -> Option<usize> {
    if line.starts_with(needle) {
        Some(0)
    } else {
        match line.find(needle) {
            Some(pos) => match line.chars().nth(pos - 1) {
                // The needle is prefixed with a separator
                Some(c) if c == ' ' || c == '\t' || c == ':' || c == '|' => Some(pos),
                // The needle is mid-line, discard it.
                _ => None,
            },
            None => None,
        }
    }
}

/// Check if a line begins a multi-line error.
fn is_multiline(line: &str) -> Option<Parser> {
    if let Some(pos) = start_find(line, "Traceback (most recent call last):") {
        Some(Parser::PythonTraceback(pos))
    } else if let Some(pos) = start_find(line, "panic:") {
        Some(Parser::GoStacktrace(pos, GoStatus::Header))
    } else {
        None
    }
}

fn go_tb_completed(pos: usize, line: &str) -> bool {
    lazy_static! {
        static ref FuncCall: Regex = Regex::new(r"^[a-z].*\(.*\)$").unwrap();
    }
    if line.len() > pos {
        let l = &line[pos..];
        !(
            l.starts_with("goroutine ") // block separator
            || l.starts_with("created by") // trace separator
            || FuncCall.is_match(l) // function call
            || l.chars().nth(pos) == Some('\t')
            // ^ call location
        )
    } else {
        false
    }
}

fn is_error_line(line: &str) -> bool {
    lazy_static! {
        static ref RE: Regex = Regex::new(concat!(
            "(?-u:(",
            // Error codes
            r#"ERROR [0-9]{4}"#,
            r#"|<title>503 Service Unavailable</title>"#,
            // Ansible errors
            r#"| ERROR$"#,
            r#"|\|   "msg": ""#,
            r#"|: FAILED!"#,
            r#"|\| FAILED \|"#,
            r#"|\| (fatal|failed|error): "#,
            r#"| The error appears to be in "#,
            r#"| failed: [1-9][0-9]*[ \t]"#,
            r#"|stderr: 'error:"#,
            // OVS
            r#"|\|WARN\|"#,
            r#"|\[EC [0-9]+\]"#,
            // Galera
            r#"| \[Error\] "#,
            // Python errors
            r#"|[0-9Z][ \t]+ERROR[ \t]+[a-zA-Z]"#,
            // tempest errors
            r#"|^FAIL: "#,
            r#"|^FAILED: "#,
            r#"|\.\.\. FAILED$"#,
            // Go errors
            r#"|\] ERROR: "#,
            // Fluentbit
            r#"|"level":"ERROR""#,
            // Kubernetes event
            r#"|Warning[ \t]+Failed[ \t]+"#,
            r#"|\bE[0-9]{4}\b"#,
            r#"|msg="error"#,
            r#"|msg="an error"#,
            r#"|"level":"error""#,
            r#"|\blevel=error\b"#,
            "))"
        ))
        .unwrap();
    }
    RE.is_match(line)
}

impl Default for State {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use crate::{Result, State};
    fn is_multiline(body: &str) -> bool {
        let mut s = State::new();
        let mut completed = false;
        for line in body.lines() {
            assert!(!completed);
            if let Result::CompletedTraceBack = s.parse(line) {
                completed = true
            }
        }
        completed
    }

    #[test]
    fn python_tb() {
        assert!(is_multiline(
            r#"
Traceback (most recent call last):
  File "test.py", line 5, in <module>
    test()
  File "test.py", line 2, in test
    raise RuntimeError("oops")
RuntimeError: oops
"#
        ));
        assert!(is_multiline(
            r#"
2025-07-07 - Traceback (most recent call last):
2025-07-07 -   File "test.py", line 7, in <module>
2025-07-07 -     raise RuntimeError("bam")
2025-07-07 - RuntimeError: bam
"#
        ));
    }

    #[test]
    fn go_tb() {
        assert!(is_multiline(
            r#"
panic: runtime error: invalid memory address or nil pointer dereference
[signal SIGSEGV: segmentation violation code=0x1 addr=0x0 pc=0x47b081]

goroutine 1 [running]:
main.main()
	test.go:14 +0x61
exit status 2
"#
        ))
    }

    #[test]
    fn test_is_error_line() {
        for line in [
            "ERROR 2002 (HY000): Can't connect to server on '127.0.0.1' (115)",
            "2025-07-07T21:21:52Z   Warning   Failed                  Pod                     logserver-0                           Error: ImagePullBackOff",
            "2025-07-07T17:03:05.595305798-04:00 stderr F time=\"2025-07-07T21:03:05Z\" level=warning msg=\"an error was encountered ",
            "2025-07-07T17:09:04.148248939-04:00 stderr F E0707 21:09:04.148229       1 queueinformer_",
            "2025-07-07T17:09:26.167025939-04:00 stderr F time=\"2025-07-07T21:09:26Z\" level=info msg=\"error updating ",
            "2025-07-07T17:02:55.673388956-04:00 stderr F time=\"2025-07-07T21:02:55Z\" level=warning msg=\"error adding",
            "2025-07-07T17:02:55.753817892-04:00 stderr F {\"level\":\"error\",\"ts\"",
            "{2} neutron.tests.unit.agent.test_plug_with_ns [0.034190s] ... FAILED",
            "E4242 oops",
            "test.go] E4242 bam",
            "13 ERROR neutron",
            "Z  ERROR  setup",
            "Z\tERROR\ttest",
            "fail level=error",
            "ovsdb_log(log_fsync3)|WARN|fsync failed (Invalid argument)",
            "BGP: [KTE2S-GTBDA][EC 100663301] INTERFACE_ADDRESS_DEL: Cannot find IF",
            "controller | controller-0 | FAILED | rc=2 >>"
        ] {
            assert!(crate::is_error_line(line), "'{}' is not an error", line);
        };

        for line in ["2025-07-07 - Running a script"] {
            assert!(!crate::is_error_line(line), "'{}' is an error", line);
        }
    }
}
