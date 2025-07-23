// Copyright (C) 2025 Red Hat
// SPDX-License-Identifier: Apache-2.0

use std::fs::File;
use systemd_journal_reader::JournalReader;

fn main() -> std::io::Result<()> {
    for argument in std::env::args().skip(1) {
        let mut journal = JournalReader::new(File::open(&argument)?)?;
        while let Some(entry) = journal.next_entry() {
            println!("{:?}", entry);
        }
        println!("{argument}");
    }
    Ok(())
}

