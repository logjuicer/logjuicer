// Copyright (C) 2022 Red Hat
// SPDX-License-Identifier: Apache-2.0

use lazy_static::lazy_static;
use regex::Regex;
use regex::Split;

fn words(line: &str) -> Split {
    lazy_static! {
        static ref RE: Regex = Regex::new(r"[ \t]+").unwrap();
    }
    RE.split(line)
}

fn is_date(word: &str) -> bool {
    lazy_static! {
        static ref RE: Regex = Regex::new(concat!(
            "(?i-u:^(",
            "sunday|monday|tuesday|wednesday|thursday|friday|saturday|",
            "january|february|march|april|may|june|july|august|september|october|november|december",
            ")$)"
        ))
        .unwrap();
    }
    RE.is_match(word)
}

fn is_error(word: &str) -> bool {
    lazy_static! {
        static ref RE: Regex = Regex::new(concat!(
            "(?i-u:^(",
            "error|failure|failed|warning|",
            "err|fail|warn|",
            "exception|traceback",
            ")$)"
        ))
        .unwrap();
    }
    RE.is_match(word)
}

fn is_uid(word: &str) -> bool {
    lazy_static! {
        static ref RE: Regex = Regex::new(concat!("(?i-u:^(", "[0-9a-fx]+[:.-]*", ")+$)")).unwrap();
    }
    RE.is_match(word)
}

fn is_url(word: &str) -> bool {
    lazy_static! {
        static ref RE: Regex =
            Regex::new(concat!("(?i:^", "(https|http|ftp|ssh)://", ")")).unwrap();
    }
    RE.is_match(word)
}

fn is_hash(word: &str) -> bool {
    lazy_static! {
        static ref RE: Regex = Regex::new(concat!("(?i:^", "(hash|sha|md)[0-9]*:", ")")).unwrap();
    }
    RE.is_match(word)
}

fn is_pubssh(word: &str) -> bool {
    word.starts_with("AAAA")
}

fn is_path(word: &str) -> bool {
    word.contains("/")
}

fn is_refs(word: &str) -> bool {
    lazy_static! {
        static ref RE: Regex = Regex::new(concat!(r"^\w{7}\.\.\w{7}$")).unwrap();
    }
    word.starts_with("refs/") || word.starts_with("repos/") || RE.is_match(word)
}

fn is_composite(word: &str) -> Option<(&str, &str)> {
    lazy_static! {
        static ref RE: Regex = Regex::new(concat!("[:=]")).unwrap();
    }
    let fields: Vec<&str> = RE.splitn(word, 2).collect();
    match fields[..] {
        [k, v] => Some((k, v)),
        _ => None,
    }
}

fn is_terminated_by_dash(word: &str) -> Option<&str> {
    let parts: Vec<&str> = word.split_inclusive('-').collect();
    match parts[..] {
        [k, _suffix] => Some(k),
        _ => None,
    }
}

fn is_key_for_id(word: &str) -> bool {
    lazy_static! {
        static ref RE: Regex =
            Regex::new(concat!("(?i:", "_(id|key|region|token|secret)", ")")).unwrap();
    }
    RE.is_match(word)
}

fn is_random_path(word: &str) -> bool {
    word.contains("/tmp")
}

#[cfg(test)]
mod re_tests {
    use super::*;

    #[test]
    fn test_date() {
        assert!(vec!["sunday", "saturday", "Monday"]
            .into_iter()
            .all(is_date));
        assert!(vec!["sunday ", " saturday", " jan ", "sund"]
            .into_iter()
            .all(|v| !is_date(v)));
    }

    #[test]
    fn test_id() {
        assert!(vec![
            "aa:bb:cc:00",
            "42.24.21.12",
            "abab-efef",
            "2022-02-03",
            "18:01:00.1"
        ]
        .into_iter()
        .all(is_uid))
    }

    #[test]
    fn test_hash() {
        assert!(vec!["sha256:aabbcc00", "md5:test", "MD42:abab",]
            .into_iter()
            .all(is_hash))
    }

    #[test]
    fn test_composite() {
        assert_eq!(is_composite("key=value"), Some(("key", "value")));
        assert_eq!(is_composite("keyvalue"), None);
    }

    #[test]
    fn test_random_path() {
        assert!(is_random_path("/tmp/test"));
        assert!(is_random_path("/var/tmp/key"));
        assert_eq!(is_random_path("/usr"), false);
    }
}

pub fn parse(word: &str) -> Option<&str> {
    if is_date(word) {
        Some("DATE")
    } else if is_hash(word) {
        Some("HASH")
    } else if is_uid(word) {
        Some("ID")
    } else if is_url(word) {
        Some("URL")
    } else if is_random_path(word) {
        Some("PATH")
    } else if is_pubssh(word) {
        Some("KEY")
    } else if is_refs(word) {
        Some("REF")
    } else {
        None
    }
}

pub fn trim_quotes(word: &str) -> Option<&str> {
    let strip = word.trim_matches(|c| c == '\'' || c == '"');
    if strip.len() < word.len() {
        Some(strip)
    } else {
        None
    }
}

pub fn trim(word: &str) -> Option<&str> {
    let strip = word.trim_matches(|c| {
        (c >= '0' && c <= '9')
            || c == '('
            || c == ')'
            || c == '['
            || c == ']'
            || c == '.'
            || c == 'x' // for hexa form
            || c == 'u' // for python unicode prefix
            || c == '{'
            || c == '}'
            || c == '>'
            || c == '<'
    });
    if strip.len() < word.len() {
        Some(strip)
    } else {
        None
    }
}

pub fn push_error(word: &str, result: &mut String) {
    // Make the error takes more space
    result.push_str(word);
    result.push_str("A ");
    result.push_str(word);
    result.push_str("B ");
    result.push_str(word)
}

/// The tokenizer main (recursive) function, which tries to remove word's noise
pub fn do_process(word: &str, result: &mut String) {
    // First we handle 3 letters word
    if word.len() <= 3 {
        // This is currently confusing the hashing vectorizer,
        // but it might be useful to keep small word for another feature vector
        // result.push_str("SML")
    } else
    // Then we try to process from the most specifics to the most general case
    if let Some(token) = parse(word) {
        // e.g. `February` or `sha256:...`
        result.push_str(token)
    } else if is_error(word) {
        // e.g. `Traceback`
        push_error(word, result)
    } else if let Some(strip) = trim_quotes(word) {
        // e.g. `"February"`
        // here we recursively call do_process to process the word inside quotes
        do_process(strip, result);
        // add a token to differentiate untrimmed words,
        // e.g. `"result": "value"` becomes `result' value"`
        result.push('\'')
    } else if let Some(strip) = trim(word) {
        // e.g. `systemd[42]`
        do_process(strip, result);
        // add a token to differentiate untrimmed words,
        // e.g. `systemd[42]` becomes `systemd%`
        result.push('%')
    } else if let Some((key, value)) = is_composite(word) {
        // e.g. TOKEN=42
        do_process(key, result);
        if is_key_for_id(key) {
            result.push_str("' KUID")
        } else {
            result.push_str("' '");
            do_process(value, result);
        }
    } else if is_path(word) {
        for component in word.split("/") {
            do_process(component, result);
            result.push_str("/ ");
        }
    } else if let Some(key) = is_terminated_by_dash(word) {
        // e.g. `object_name-eabab`
        result.push_str(key);
        result.push_str("-ID");
    } else if word.len() >= 32 {
        result.push_str("BIG")
    } else {
        result.push_str(word)
    }
}

/// The tokenizer entry point, call do_process per words
pub fn process(line: &str) -> String {
    let mut result = String::with_capacity(line.len());
    for word in words(line) {
        do_process(word, &mut result);
        result.push(' ')
    }
    result.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_process() {
        assert_eq!(
            process("error hash mismatch 'sha256:42'"),
            "errorA errorB error hash mismatch HASH'"
        );
        assert_eq!(
            process("getting \"http://local:4242/test\""),
            "getting URL'"
        );
        assert_eq!(
            process("sha256://toto tata finished in 28ms by systemd[4248]"),
            "HASH tata finished  %  systemd%"
        );
        assert_eq!(
            process("log_url=https://ansible AWS_ACCESS_KEY_ID=ASIA6CCDWXDODS7A4X53 "),
            "log_url' 'URL AWS_ACCESS_KEY_ID\' KUID%"
        );
        assert_eq!(
            process("Event ID: 3e75e420-761f-11ec-8d18-a0957bd68c36"),
            process("Event ID: f671eb00-730e-11ec-915f-abcd86bae8f1")
        );
        assert_eq!(
            process("File \"nodepool/cmd/config_validator.py\", line 144, in validate"),
            "File nodepool/ / config_validator.py\",/ ' line %  validate"
        );
    }

    #[test]
    fn test_words() {
        assert_eq!(
            words(" a b ").collect::<Vec<&str>>(),
            vec!["", "a", "b", ""]
        );
    }
}
