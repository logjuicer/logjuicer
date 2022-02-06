// Copyright (C) 2022 Red Hat
// SPDX-License-Identifier: Apache-2.0

//! A cli demo to run the tokenizer on stdin.
use logreduce_tokenizer::{tokenizer};

use std::io::Write;
use std::io::{self};

fn main() -> io::Result<()> {
    let stdin = std::io::stdin();
    let mut line = String::with_capacity(1024);

    let stdout = std::io::stdout();
    let mut lock = stdout.lock();

    while stdin.read_line(&mut line)? != 0 {
        writeln!(lock, "{}", tokenizer::process(&line))?;
        line.clear();
    }
    Ok(())
}
