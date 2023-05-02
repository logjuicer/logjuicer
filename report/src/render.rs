// Copyright (C) 2022 Red Hat
// SPDX-License-Identifier: Apache-2.0

fn render(fp: &std::path::Path) {
    let report = logreduce_model::Report::load(fp).unwrap();
    let mut html = fp.to_path_buf();
    html.set_extension("html");
    std::fs::write(&html, logreduce_report::render(&report).unwrap()).unwrap();
    println!("Updated {:?}", html);
}

fn main() {
    match std::env::args().collect::<Vec<String>>().as_slice() {
        [_, fp] => render(std::path::Path::new(fp)),
        _ => println!("usage: report.json"),
    }
}
