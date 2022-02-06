// Copyright (C) 2022 Red Hat
// SPDX-License-Identifier: Apache-2.0

//! The tokenizer logic
//!
//! The main function is [process]. The output is designed for further feature extraction,
//! for example with a bag of words or hashing vectorizer. It looks like this:
//!
//! ```rust
//! # use logreduce_tokenizer::tokenizer::{process};
//! assert_eq!(process(
//!    "2017-06-24 02:52:17.732 22627 tempest.lib.common.rest_client [req-b932e095-6706-4f5a-bd75-241c407a9d01 ] Request (main): 201 POST https://10.0.1.9/identity/v3/auth/tokens"),
//!    "%ID %ID %ID tempest.lib.common.rest_client %BIG%NUM  Request main%NUM%EQ %NUM  POST %URL")
//! ```
//!
//! Here are some use cases:
//!
//! ```rust
//! # use logreduce_tokenizer::{tokens_eq, tokenizer::*};
//! tokens_eq!("+ export ZUUL_REF=refs/zuul/master/6546b192211a4531859db9d8b9375154",
//!            "+ export ZUUL_REF=refs/zuul/master/9249f6066a2041bbbeb838e2ca1cf2b4");
//! tokens_eq!("2017-06-23 20:10:06,848 INFO:dlrn-build:DEBUG: writing output... [ 90%] configuration",
//!            "2017-06-24 13:35:57,754 INFO:dlrn-build:DEBUG: writing output... [ 88%] configuration");
//! tokens_eq!("tempest.lib.common.rest_client [req-b932e095-6706-4f5a-bd75-241c407a9d01 ] Request (main): 201 POST https://10.0.1.9/identity/v3/auth/tokens",
//!            "tempest.lib.common.rest_client [req-08043549-3227-4c61-aa3b-9d02fc8437c3 ] Request (main): 201 POST https://104.130.217.34/identity/v3/auth/tokens");
//! ```
//!
//! TODO: decode json object and re-order the key to pass this test:
//! ```should_panic
//! # use logreduce_tokenizer::tokenizer::{process};
//! assert_eq!(process("{\"key\": true, \"oth\": 1}"), process("{\"oth\": 1, \"key\": true}"));
//! ```


use lazy_static::lazy_static;
use regex::Regex;
use regex::Split;

fn words(line: &str) -> Split {
    lazy_static! {
        static ref RE: Regex = Regex::new(r"[ \t]+").unwrap();
    }
    RE.split(line)
}

/// Apply global filter to skip specific lines.
/// ```rust
/// # use logreduce_tokenizer::tokenizer::{process};
/// assert_eq!(process("iptables -N RULES42 -L"), "%GL_FILTER");
/// ```
fn global_filter(line: &str) -> bool {
    lazy_static! {
        static ref RE: Regex = Regex::new(concat!(
            r"GET / HTTP/1.1",
            // yum mirrors information
            r"|\* [a-zA-Z]+: [a-zA-Z0-9\.-]*$|Trying other mirror.",
            // useless debug statement
            r"|ovs-ofctl .* (dump-ports|dump-flows|show)\b",
            r"|(ip|eb)tables .* -L\b",
        )).unwrap();
    }
    line.len() < 5 || RE.is_match(line)
}

/// Replace numbers sequences with `N`.
/// ```rust
/// # use logreduce_tokenizer::{tokens_eq, tokenizer::*};
/// tokens_eq!("running test42", "running test43");
/// ```
fn remove_numbers(word: &str) -> String {
    lazy_static! {
        static ref RE: Regex = Regex::new("[0-9]+").unwrap();
    }
    RE.replace_all(word, "N").to_string()
}

/// Check if a word matches a date.
/// ```rust
/// # use logreduce_tokenizer::{tokens_eq, tokenizer::*};
/// tokens_eq!("Sunday February 6th - message", "Monday February 7th - message");
/// ```
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

/// Check if a word matches an error prefix.
fn is_error(word: &str) -> bool {
    lazy_static! {
        static ref RE: Regex = Regex::new(concat!(
            "(?i-u:^(",
            "error|failure|failed|warning|",
            "err|fail|warn|",
            "assert|assertion|non-zero|",
            "exception|traceback",
            ")$)"
        ))
        .unwrap();
    }
    RE.is_match(word)
}

/// Check if a word contains weird char, likely in generated id.
/// ```rust
/// # use logreduce_tokenizer::{tokens_eq, tokenizer::*};
/// tokens_eq!("A{$@42", "$A%TE");
/// ```
fn contains_odd_char(word: &str) -> bool {
    lazy_static! {
        static ref RE: Regex = Regex::new(r"[<>{}%$,*]").unwrap();
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

/// ```
/// # use logreduce_tokenizer::{tokens_eq, tokenizer::{process}};
/// tokens_eq!("md5:d41d8cd98f00b204e9800998ecf8427e", "md5:e7b26fc34f528b5b19c4450867b9d597")
/// ```
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

/// ```
/// # use logreduce_tokenizer::{tokens_eq, tokenizer::{process}};
/// tokens_eq!("key=01:02:ff", "key=aa:bb:cc")
/// ```
fn is_key_value(word: &str) -> Option<(&str, &str)> {
    lazy_static! {
        static ref RE: Regex = Regex::new(concat!("[:=]")).unwrap();
    }
    let fields: Vec<&str> = RE.splitn(word, 2).collect();
    match fields[..] {
        [k, v] => {
            if k.starts_with(|c| (c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z') || (c == '_')) {
                Some((k, v))
            } else {
                None
            }
        }
        _ => None,
    }
}

/// ```
/// # use logreduce_tokenizer::{tokens_eq, tokenizer::{process}};
/// tokens_eq!("port-ea42", "port-eb51");
/// tokens_eq!("test-copysrc", "dat-copysrc");
/// ```
fn is_terminated_by_dash(word: &str) -> Option<&str> {
    let parts: Vec<&str> = word.split_inclusive('-').collect();
    match parts[..] {
        [k, suffix] => {
            if suffix.starts_with("copy") {
                // buckets has the id before the dash (e.g. `uuid-copy`)
                Some(suffix)
            } else {
                Some(k)
            }
        }
        _ => None,
    }
}

fn is_key_for_id(word: &str) -> bool {
    lazy_static! {
        static ref RE: Regex =
            Regex::new(concat!("(?i:", "_(id|key|ref|region|token|secret)", ")")).unwrap();
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
    fn test_remove_numbers() {
        assert_eq!(remove_numbers("test42-check"), "testN-check");
    }

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
    fn test_is_error() {
        assert!(is_error("FAIL"));
    }

    #[test]
    fn test_id() {
        assert!(vec![
            "aa:bb:cc:00:ff",
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
        assert_eq!(is_key_value("key=value"), Some(("key", "value")));
        assert_eq!(is_key_value("keyvalue"), None);
        assert_eq!(is_key_value("!KEY=value"), None);
    }

    #[test]
    fn test_random_path() {
        assert!(is_random_path("/tmp/test"));
        assert!(is_random_path("/var/tmp/key"));
        assert_eq!(is_random_path("/usr"), false);
    }

    #[test]
    fn test_trim_pid() {
        assert_eq!(trim_pid("systemd[42]"), Some("systemd"))
    }
}

fn parse_literal(word: &str) -> Option<&str> {
    if is_date(word) {
        Some("%DATE")
    } else if is_hash(word) {
        Some("%HASH")
    } else if is_uid(word) {
        Some("%ID")
    } else if is_url(word) {
        Some("%URL")
    } else if is_random_path(word) {
        Some("%PATH")
    } else if is_pubssh(word) {
        Some("%KEY")
    } else if is_refs(word) {
        Some("%REF")
    } else {
        None
    }
}

fn trim_quotes(word: &str) -> Option<&str> {
    let strip = word
        .trim_start_matches("u\"")
        .trim_start_matches("u'")
        .trim_matches(|c| c == '\'' || c == '"' || c == ',');
    if strip.len() < word.len() {
        Some(strip)
    } else {
        None
    }
}

fn trim_pid(word: &str) -> Option<&str> {
    match word.strip_suffix("]") {
        Some(word) => word
            .trim_end_matches(|c| c >= '0' && c <= '9')
            .strip_suffix("["),
        None => None,
    }
}

fn trim_num_and_sep(word: &str) -> Option<&str> {
    let strip = word.trim_matches(|c| {
        (c >= '0' && c <= '9')
            || c == '('
            || c == ')'
            || c == '['
            || c == ']'
            || c == '.'
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

/// Makes error token appears bigger.
/// ```rust
/// # use logreduce_tokenizer::tokenizer::*;
/// assert_eq!(process("Test Fail"), "Test Fail Fail%A Fail%B Fail%C Fail%D");
/// ```
fn push_error(word: &str, result: &mut String) {
    // Make the error takes more space
    result.push_str(word);
    result.push(' ');
    result.push_str(word);
    result.push_str("%A ");
    result.push_str(word);
    result.push_str("%B ");
    result.push_str(word);
    result.push_str("%C ");
    result.push_str(word);
    result.push_str("%D ");
}

/// The tokenizer main (recursive) function
fn do_process(word: &str, result: &mut String) {
    // First we handle 3 letters word
    if word.len() <= 3 {
        // This is currently confusing the hashing vectorizer,
        // but it might be useful to keep small words for another feature vector
        // result.push_str("SML")
    } else
    // Then we try to process from the most specifics to the most general case
    if let Some(token) = parse_literal(word) {
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
    } else if let Some(strip) = trim_pid(word) {
        // e.g. `"systemd[42]"`
        do_process(strip, result);
        result.push_str("%PID");
    } else if contains_odd_char(word) {
        result.push_str("%ODD")
    } else if let Some((key, value)) = is_key_value(word) {
        // e.g. TOKEN=42
        do_process(key, result);
        if is_key_for_id(key) {
            result.push_str("%EQ %VALUE_ID")
        } else {
            result.push_str("%EQ ");
            do_process(value, result);
        }
    } else if is_path(word) {
        for component in word.split("/") {
            do_process(component, result);
            result.push_str("/ ");
        }
    } else if let Some(strip) = trim_num_and_sep(word) {
        do_process(strip, result);
        result.push_str("%NUM")
    } else if let Some(key) = is_terminated_by_dash(word) {
        // e.g. `object_name-eabab`
        result.push_str(&remove_numbers(key));
        result.push_str("%DASH_ID");
    } else if word.len() >= 32 {
        result.push_str("%BIG")
    } else {
        result.push_str(&remove_numbers(word))
    }
}

/// The tokenizer entry point
pub fn process(line: &str) -> String {
    // the current logreduce process provides cr terminated line
    // and it's easier to remove it here to avoid regexp confusion.
    let line = line.trim_end_matches(|c| c == '\n' || c == '\r');

    // check for global filter first
    if global_filter(line) {
        return "%GL_FILTER".to_string();
    }

    // split the line into space separated words.
    let mut result = String::with_capacity(line.len());
    for word in words(line) {
        do_process(word, &mut result);
        result.push(' ')
    }
    result.trim().to_string()
}

/// Helper macro to write short tests. `tokens_eq!("a", "b")` is `assert_eq!(process("a"), process("b"))`
#[macro_export]
macro_rules! tokens_eq {
    // macth like arm for macro
    ($a:expr,$b:expr) => {
        assert_eq!(process($a), process($b))
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_process_nl() {
        assert_eq!(process("testy\r\n"), "testy");
        assert_eq!(process("* mirror: 42\n"), "%GL_FILTER");
    }

    #[test]
    fn test_process() {
        assert_eq!(
            process("error hash mismatch 'sha256:42'"),
            "error error%A error%B error%C error%D  hash mismatch %HASH'"
        );
        assert_eq!(
            process("getting \"http://local:4242/test\""),
            "getting %URL'"
        );
        assert_eq!(
            process("sha256://toto tata finished in 28ms by systemd[4248]"),
            "%HASH tata finished  %NUM  systemd%PID"
        );
        assert_eq!(
            process("log_url=https://ansible AWS_ACCESS_KEY_ID=ASIA6CCDWXDODS7A4X53 "),
            "log_url%EQ %URL AWS_ACCESS_KEY_ID%EQ %VALUE_ID"
        );
        assert_eq!(
            process("Event ID: 3e75e420-761f-11ec-8d18-a0957bd68c36"),
            process("Event ID: f671eb00-730e-11ec-915f-abcd86bae8f1")
        );
        assert_eq!(
            process("\"mac_address\": \"12:fa:c8:b2:e0:ff\","),
            process("\"mac_address\": \"12:a6:f2:17:d3:b5\",")
        );
        assert_eq!(
            process("File \"nodepool/cmd/config_validator.py\", line 144, in validate"),
            "File nodepool/ / config_validator.py/ ' line '  validate"
        );
        assert_eq!(
            process("controller |             \"after\": \"3}QP5CJuNBP65S%c:y>o\"",),
            "controller  after'%EQ ' %ODD'"
        );
        assert_eq!(
            process("[Zuul] Job complete, result: FAILURE"),
            "Zuul%NUM  complete' result%EQ  FAILURE FAILURE%A FAILURE%B FAILURE%C FAILURE%D"
        );
        assert_eq!(
            process("controller | +3}QP5CJuNBP65S%c:y>o"),
            process("controller | +1T9,Eqb@g[VL@b0u*Et!")
        );
        assert_eq!(
            process("   \"contents\": \"3}QP5CJuNBP65S%c:y>o\""),
            process("   \"contents\": \"U%aNO^b5ITFU^xTTa9rV\",")
        );
        tokens_eq!(
            "ZUUL_REF=Z60f0ad207fbb4c55a07d665ef44131a4",
            "ZUUL_REF=Zbffe5ccbe3ef4ab48c016783ea185dfa"
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
