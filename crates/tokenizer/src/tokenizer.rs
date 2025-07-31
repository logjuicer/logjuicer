// Copyright (C) 2022 Red Hat
// SPDX-License-Identifier: Apache-2.0

#![warn(missing_docs)]
#![allow(clippy::manual_range_contains)]

//! This library provides a tokenizer function for the [logjuicer](https://github.com/logjuicer/logjuicer) project.
//!
//! The goal is to replace varying words with fixed tokens (e.g. `sha256://...` is converted to `%HASH`).
//!
//! The main function is [process]. The output is designed for further feature extraction,
//! for example with a bag of words or hashing vectorizer. It looks like this:
//!
//! ```rust
//! # use logjuicer_tokenizer::{process};
//! assert_eq!(process(
//!    "2017-06-24 02:52:17.732 22627 tempest.lib.common.rest_client [req-b932e095-6706-4f5a-bd75-241c407a9d01 ] Request (main): 201 POST https://10.0.1.9/identity/v3/auth/tokens"),
//!    "%ID %ID %ID tempest.lib.common.rest_client %COOKIE Request main%EQ %ID POST %URL")
//! ```
//!
//! Here are some use cases:
//!
//! ```rust
//! # use logjuicer_tokenizer::*;
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
//! # use logjuicer_tokenizer::{process};
//! assert_eq!(process("{\"key\": true, \"oth\": 1}"), process("{\"oth\": 1, \"key\": true}"));
//! ```

use lazy_static::lazy_static;
use regex::Regex;
use regex::Split;

pub mod index_name;

fn words(line: &str) -> Split {
    lazy_static! {
        static ref RE: Regex = Regex::new(r"([ \t]|\\[nr])+").unwrap();
    }
    RE.split(line)
}

fn trim_quote_and_punctuation(word: &str) -> &str {
    word.trim_start_matches("u\"")
        .trim_start_matches("u'")
        .trim_matches(|c| {
            matches!(
                c,
                '\'' | '"' | ',' | ';' | '(' | ')' | '[' | ']' | '{' | '}' | '<' | '>' | '\\'
            )
        })
}

/// Apply global filter to skip specific lines.
fn global_filter(line: &str) -> bool {
    lazy_static! {
        static ref RE: Regex = Regex::new(concat!(
            r"GET / HTTP/1.1",
            // yum mirrors information
            r"|\* [a-zA-Z]+: [a-zA-Z0-9\.-]*$|Trying other mirror.",
            // useless debug statement
            r"|ovs-ofctl .* (dump-ports|dump-flows|show)\b",
            r"|(ip|eb)tables .* -L\b",
            // chrony logs
            r"|(^\^[+*-] [a-z0-9\.>-]{5,} [0-9])",
            // dnsmasq
            r"|dnsmasq(\[[0-9]+\])?: (query|forwarded|reply|cached|config)",
            // memcached logs
            r"|(^[a-f0-9s/]+>[0-9]+ )",
            // shell debugs
            r"|(^\+\+ echo [^ ]+$)",
            // sysctl taps
            r"|(^net.ipv[46].(conf|neigh).tap)",
            r#"|(^[" \t]*net.interface.tap)"#,
            // key's randomart
            r#"|([ '",]*\|.{17}\|[ '",]*$)"#
        )).unwrap();
    }
    let is_single_word = !line.contains(|c: char| c.is_whitespace());
    is_single_word || RE.is_match(line)
}
#[test]
fn test_global_filter() {
    assert_eq!(process("iptables -N RULES42 -L"), "%GL_FILTER");
    assert_eq!(
        process("crc dnsmasq[108501]: query[AAAA] no-such-master from 192.168.122.100"),
        "%GL_FILTER"
    );
    assert_eq!(
        process("crc dnsmasq: reply example.com is NODATA-IPv6"),
        "%GL_FILTER"
    );
    assert_eq!(
        process("e2b607f0bb193c9bfed94af532ba1>33 STORED"),
        "%GL_FILTER"
    );
    assert_eq!(process("s/5bf8>28 sending key"), "%GL_FILTER");
    assert_eq!(
        process("^- srcf-ntp.example.edu 2 9 377 429 -358us[ -358us] +/- 63ms"),
        "%GL_FILTER"
    );
    assert_eq!(process("++ echo mswAxrrS1YwyGtIut9Vd"), "%GL_FILTER");
    assert!(global_filter("|        =+ooo=+.o|"));
    assert!(global_filter("hostname: |.o.B ..+S        |"));
    assert!(global_filter("                    \"|           oo... |\""));
}

/// Replace numbers sequences with `N`.
fn remove_numbers(word: &str) -> String {
    lazy_static! {
        static ref RE: Regex = Regex::new("([0-9]+\\.[0-9]+)|([0-9]+)").unwrap();
    }
    RE.replace_all(word, "N").to_string()
}
#[test]
fn test_remove_numbers() {
    tokens_eq!("running test4.2", "running test43");
}

/// Check if a word matches a date.
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
#[test]
fn test_is_date() {
    tokens_eq!(
        "Sunday February 6th - message",
        "Monday February 7th - message"
    );
}

/// Check if a word matches an error prefix.
fn is_error(word: &str) -> bool {
    lazy_static! {
        static ref RE: Regex = Regex::new(concat!(
            "(?i-u:^(",
            "error|fatal|failure|failed|warning|",
            "err|fail|warn|",
            "denied|",
            "assert|assertion|non-zero|",
            "exception|traceback",
            ")$)"
        ))
        .unwrap();
    }
    RE.is_match(word)
}

/// Check if a word contains weird char, likely in generated id.
fn contains_odd_char(word: &str) -> bool {
    lazy_static! {
        static ref RE: Regex = Regex::new(r"[<>{}%$,*]").unwrap();
    }
    RE.is_match(word)
}

#[test]
fn test_contains_odd_char() {
    tokens_eq!("A{$@42", "$A%TE");
}

fn is_lowercase_consonant(c: char) -> bool {
    matches!(c, 'b'..='d' | 'f'..='h' | 'j'..='n' | 'p'..='t' | 'v'..='x' | 'z')
}

fn contains_no_vowel(word: &str) -> bool {
    let mut found = false;
    for c in word.chars().map(|c| c.to_ascii_lowercase()) {
        if crate::index_name::is_lowercase_vowel(c) {
            return false;
        } else if is_lowercase_consonant(c) {
            found = true;
        }
    }
    found
}

/// Check if a word only contains hexa and sep char, or if it only contains consonants.
fn is_uid(word: &str) -> bool {
    lazy_static! {
        static ref RE: Regex = Regex::new(concat!(
            "^(:*",
            r"[\[\]0-9a-fA-FxZ,]+[:.-]*",
            r"|rabbitmq-cluster-id-.*",
            ")+$"
        ))
        .unwrap();
    }
    RE.is_match(word) || contains_no_vowel(word)
}

#[test]
fn test_is_uid() {
    tokens_eq!("the_ip is 127.0.0.1", "the_ip is ::1");
    tokens_eq!("the_mac is aa:bb:cc", "the_mac is 00:11:cc");
    tokens_eq!("the_num is 0x4243", "the_num is 0x4142");
    tokens_eq!(
        "internal_cluster_id \"rabbitmq-cluster-id-WL19_cCo6Ttpy8mXLuPZ9g\"",
        "internal_cluster_id \"rabbitmq-cluster-id-WM19-cCo6Ttpy8mXLuPZ8g\""
    );
}

/// 3 x 4letters word separated by -
fn is_uuid(word: &str) -> bool {
    lazy_static! {
        static ref RE: Regex = Regex::new(concat!(
            "^",
            "[a-zA-Z0-9].*-[a-zA-Z0-9]{4}-[a-zA-Z0-9]{4}-[a-zA-Z0-9]{4}-",
            "$"
        ))
        .unwrap();
    }
    RE.is_match(word)
}

/// 3 dash separator
fn has_many_dash(word: &str) -> bool {
    lazy_static! {
        static ref RE: Regex = Regex::new(concat!("^.+-.+-.+-.")).unwrap();
    }
    RE.is_match(word)
}

fn is_cookie(word: &str) -> bool {
    lazy_static! {
        static ref RE: Regex =
            Regex::new(concat!("^(", "gAAAA|AAAA|tx[a-z]|tap|req-|AUTH_", ")")).unwrap();
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

fn is_base64(word: &str) -> bool {
    lazy_static! {
        static ref RE: Regex = Regex::new(concat!("^", "[A-Za-z0-9+/=]+", "$")).unwrap();
    }
    word.ends_with("==") || (word.len() > 24 && (word.ends_with("=") || RE.is_match(word)))
}

fn is_systemd_unit_container_pid(word: &str) -> bool {
    lazy_static! {
        static ref RE: Regex = Regex::new(concat!("^", "[a-z]+_[a-z]+\\[[0-9]+\\]:", "$")).unwrap();
    }
    RE.is_match(word)
}

#[test]
fn test_is_base64() {
    tokens_eq!(
        "MqoplXLA2LPnJKTNMQW5JpGyMLJcLxRDDEejzh6b1im8KV/5TRKDsg7b5FwBJJoN",
        "fJkzOzsJdqxvhSvDFkUlAP7a/+kOBCYi1Yp1pz0v/mHLi0r1z5xtx3BemXVYHbom"
    );
    tokens_eq!("a EqTsSXKlOsEjfIdFld+uwopnIIqvKI+Xu6e0RcAGYJEfj56/MG2IdH7/h1JmQ///\\nn2RZ/ocRcL5as2EHQES0b+/I12a2Gj+W+ub0OQAGDq8iL5o8P0/ogEWrpZmoBC+oi",
            "a MqoplXLA2LPnJKTNMQW5JpGyMLJcLxRDDEejzh6b1im8KV/5TRKDsg7b5FwBJJoN fJkzOzsJdqxvhSvDFkUlAP7a/+kOBCYi1Yp1pz0v/mHLi0r1z5xtx3BemXVYHbom");
    tokens_eq!(
        "\"ssh_host_key_ecdsa_public\": \"AAAAE2VjZHNhLXNoYTItbmlzdHAyNTYAAAAIbmlzdHAyNTYAAABBBAoR7WEHBOBURhlsegwrbX2xTC/UFVwNR6Q4RBOcWPcUNpTbgmMZ8vhNWqnzrL/NXMWuHqrXECCyBqgtethMuPg=\"",
        "\"ssh_host_key_ecdsa_public\": \"AAAAE2VjZHNhLXNoYTItbmlzdHAyNTYAAAAIbmlzdHAyNTYAAABBBPaZ3NnBO+oUoGDFu3xXcxwe4KRghJTOj5y/n+GojwicVwHC7JEYVmZcPksW/kcFfy7uq/JkuIA1j7tUxfiMuRY=\""
    );
    tokens_eq!(
        "\"ssh_host_key_ed25519_public\": \"AAAAC3NzaC1lZDI1NTE5AAAAIDoRunCDSjliGLhWFeZDJ2Zysc1E/3ri+aHA+W467hxc\"",
        "\"ssh_host_key_ed25519_public\": \"AAAAC3NzaC1lZDI1NTE5AAAAIB++yyvs20oahbmnYE2RJqBzXBNxL1zVYMf0MiHreF33\""
    )
}

fn is_hash(word: &str) -> bool {
    lazy_static! {
        static ref RE: Regex = Regex::new(concat!(
            "(?i:^",
            "(hash|sha|md)[0-9]*[:~]",
            ")|",
            // csrf tokens
            "\\.?[a-zA-Z0-9_+/-]{64,}?"
        ))
        .unwrap();
    }
    !word.starts_with('/') && RE.is_match(word)
}

#[test]
fn test_is_hash() {
    tokens_eq!(
        "md5:d41d8cd98f00b204e9800998ecf8427e",
        "md5:e7b26fc34f528b5b19c4450867b9d597"
    );
    assert!(is_hash(
        "sha256~fDvjOUfdzu5KKztYJO98QqiOQFiSp2sSPQjEE2SexmE"
    ));
    assert!(is_hash(
        "zjxRGFLA4ZVTXXSKpL_U37kHYHoyJ25GcMqoN27A5OS4PodEjDomArnq_36WggVk"
    ));
    assert!(is_hash(".eJw1j81OwkAURl-lmTVNZu78dbojUSEKagQB3TTTuXcQkBZKSUTCu1NiXH6b851zZkU4NLFo6w1VLGe_65-3wcOorz5n"));
}

fn is_refs(word: &str) -> bool {
    lazy_static! {
        static ref RE: Regex = Regex::new(concat!(r"^\w{7}\.\.\w{7}$")).unwrap();
    }
    word.starts_with("refs/") || word.starts_with("repos/") || RE.is_match(word)
}

fn is_key_value(word: &str) -> Option<(&str, &str)> {
    match word.split_once(['=', ':']) {
        Some((k, v)) => {
            if k.starts_with(|c| (c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z') || (c == '_')) {
                Some((k, v))
            } else {
                None
            }
        }
        _ => None,
    }
}
#[test]
fn test_is_key_value() {
    tokens_eq!("key=01:02:ff", "key=aa:bb:cc")
}

/// Separate attached words like `DHCPOFFER(ipaddr)` in `DHCPOFFER ipaddr`
fn is_two_words(word: &str) -> Option<(&str, &str)> {
    word.split_once(['[', '(', '\\', '@'])
        .map(|(k, v)| (k, v.trim_end_matches([']', ')'])))
}

fn is_key_for_id(word: &str) -> bool {
    lazy_static! {
        static ref RE: Regex = Regex::new(concat!(
            "(?i:",
            "(id|key|ref|region|token|secret|password|pipeline)",
            ")"
        ))
        .unwrap();
    }
    RE.is_match(word)
}

fn is_password_key(word: &str) -> bool {
    word.ends_with("password:") || word.ends_with("password=")
}

fn is_random_path(word: &str) -> bool {
    word.contains("tmp/") || word.contains("/tmp") || word.starts_with("tmp")
}
#[test]
fn test_is_random_path() {
    tokens_eq!(
        "'_original_basename': 'tmpmh4nrjbd'",
        "'_original_basename': 'tmp7v726n_c'"
    )
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
        assert!(IntoIterator::into_iter(["sunday", "saturday", "Monday"]).all(is_date));
        assert!(
            IntoIterator::into_iter(["sunday ", " saturday", " jan ", "sund"]).all(|v| !is_date(v))
        );
    }

    #[test]
    fn test_is_error() {
        assert!(is_error("FAIL"));
    }

    #[test]
    fn test_is_systemd_pid() {
        assert!(is_systemd_unit_container_pid("elastic_mirzakhani[36129]:"))
    }

    #[test]
    fn test_id() {
        assert!(IntoIterator::into_iter([
            "aa:bb:cc:00:ff",
            "42.24.21.12",
            "abab-efef",
            "2022-02-03",
            "18:01:00.1"
        ])
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
        assert!(!is_random_path("/usr"));
    }

    #[test]
    fn test_trim_pid() {
        assert_eq!(trim_pid("systemd[42"), Some("systemd"))
    }
}

fn parse_literal(word: &str) -> Option<&str> {
    if is_date(word) {
        Some("%DATE")
    } else if is_hash(word) {
        Some("%HASH")
    } else if is_uid(word) {
        Some("%ID")
    } else if is_cookie(word) {
        Some("%COOKIE")
    } else if is_uuid(word) {
        Some("%UID")
    } else if is_url(word) {
        Some("%URL")
    } else if is_random_path(word) {
        Some("%PATH")
    } else if is_refs(word) {
        Some("%REF")
    } else if is_base64(word) {
        Some("%BASE64")
    } else if is_systemd_unit_container_pid(word) {
        // systemd unit are often random because of container.
        Some("%UNIT")
    } else {
        None
    }
}

fn trim_pid(word: &str) -> Option<&str> {
    word.trim_end_matches(|c| c >= '0' && c <= '9')
        .strip_suffix('[')
}

/// Makes error token appears bigger.
fn push_error(word: &str, result: &mut String) {
    // Make the error takes more space
    result.push_str(word);
    result.push(' ');
    for id in ["%A ", "%B ", "%C ", "%D"] {
        result.push_str(word);
        result.push_str(id);
    }
}

#[test]
fn test_push_error() {
    assert_eq!(
        process("Test Fail"),
        "Test Fail Fail%A Fail%B Fail%C Fail%D"
    );
}

/// The tokenizer main (recursive) function
fn do_process(base_word: &str, iter: &mut Split, result: &mut String) -> bool {
    let word = trim_quote_and_punctuation(base_word);
    let mut added = true;
    // We try to process from the most specifics to the most general case
    if word.is_empty() {
        added = false
    } else if let Some(token) = parse_literal(word) {
        // e.g. `February` or `sha256:...`
        result.push_str(token)
    } else if is_error(word) {
        // e.g. `Traceback`
        push_error(word, result)
    } else if word.len() <= 3 {
        // This is currently confusing the hashing vectorizer,
        // but it might be useful to keep small words for another feature vector
        // result.push_str("SML")
        added = false;
    } else if let Some(strip) = trim_pid(word) {
        // e.g. `"systemd[42]"`
        do_process(strip, iter, result);
        result.push_str("%PID");
    } else if contains_odd_char(word) {
        result.push_str("%ODD")
    } else if let Some((key, value)) = is_key_value(word) {
        // e.g. TOKEN=42
        do_process(key, iter, result);
        if is_key_for_id(key) {
            if value.is_empty() {
                // Consume the next word
                let _ = iter.next();
            }
            result.push_str("%EQ %VALUE_ID")
        } else {
            result.push_str("%EQ ");
            added = do_process(value, iter, result)
        }
    } else if let Some((w1, w2)) = word.split_once('/') {
        if do_process(w1, iter, result) {
            result.push_str("/ ");
        }
        added = do_process(w2, iter, result);
    } else if let Some((w1, w2)) = word.split_once('-') {
        if has_many_dash(w2) {
            // when word contains more than 4 dash, then consider it noise.
            // e.g. heat uid looks like: undercloud-UndercloudServiceChain-dt26w6s63vd6-ServiceChain-dxxxgncfjqeg-0-yhtbooauehxj
            result.push_str("%DASH")
        } else {
            if do_process(w1, iter, result) {
                result.push_str("- ");
            }
            added = do_process(w2, iter, result)
        }
    } else if let Some((w1, w2)) = word.split_once('|') {
        if do_process(w1, iter, result) {
            result.push_str("| ");
        }
        added = do_process(w2, iter, result)
    } else if word.len() >= 32 {
        result.push_str("%BIG")
    } else if let Some((w1, w2)) = is_two_words(word) {
        if do_process(w1, iter, result) {
            result.push(' ');
        }
        added = do_process(w2, iter, result);
    } else {
        // here finally the word is added
        let x = remove_numbers(word);
        if is_password_key(&x) {
            // Consume the next word
            let _ = iter.next();
            result.push_str(&x)
        } else if x.len() > 3 {
            result.push_str(&x)
        } else {
            added = false;
        }
    }
    added
}

/// The tokenizer entry point
pub fn process(line: &str) -> String {
    // Remove surrounding whitespaces
    let line = line.trim();

    // check for global filter first
    if global_filter(line) {
        return "%GL_FILTER".to_string();
    }

    // split the line into space separated words.
    let mut result = String::with_capacity(line.len());
    let mut iter = words(line);
    while let Some(word) = iter.next() {
        if do_process(word, &mut iter, &mut result) {
            result.push(' ')
        }
    }
    // TODO: check if result contains at least 2 word
    result.truncate(result.trim_end().len());
    result
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
        assert_eq!(process("testy\r\n"), "%GL_FILTER");
        assert_eq!(process("* mirror: 42\n"), "%GL_FILTER");
    }

    #[test]
    fn test_process() {
        assert_eq!(
            process("error hash mismatch 'sha256:42'"),
            "error error%A error%B error%C error%D hash mismatch %HASH"
        );
        assert_eq!(
            process("getting \"http://local:4242/test\""),
            "getting %URL"
        );
        assert_eq!(
            process("sha256://toto tata finished in 28ms by systemd[4248]"),
            "%HASH tata finished %ID systemd%PID"
        );
        assert_eq!(
            process("log_url=https://ansible AWS_ACCESS_KEY_ID=ASIA6CCDWXDODS7A4X53 "),
            "log_url%EQ %URL AWS_ACCESS_KEY_ID%EQ %VALUE_ID"
        );
        assert_eq!(
            process("** 192.168.24.1:8787/tripleovictoria/openstack-heat-api:175194d1801ec25367354976a18e3725-updated-20220125105210 **"),
            "%ID/ tripleovictoria/ openstack- heat- %EQ %ID- updated- %ID"
        );
    }
    #[test]
    fn test_process02() {
        assert_eq!(
            process("nova::placement::password: UIbv1LPZWIXpBtaToNzsmgZI3"),
            "nova%EQ :placement::password:"
        );
        assert_eq!(
            process("2022-01-25 12:11:14 | ++ export OS_PASSWORD=PobDt1cxalvf40uv9Om5VTNkw"),
            "%ID %ID export OS_PASSWORD%EQ %VALUE_ID"
        );
        assert_eq!(
            process("^+ ntp1a.example.com 1 10 377 635 -1217us[-1069us] +/- 16ms"),
            "%GL_FILTER"
        );
        assert_eq!(process("a PobDt1cxalvf40uv9Om5VTNkw"), "%ID %BASE64");
    }

    #[test]
    fn test_process03() {
        assert_eq!(
            process("2022-01-25T14:09:24.422Z|00014|jsonrpc|WARN|tcp:[fd00:fd00:fd00:2000::21e]:50504: receive error: Connection reset by peer"),
            "%ID- %ID- %ID| %ID| jsonrpc| WARN WARN%A WARN%B WARN%C WARN%D| %ID%EQ %ID receive error error%A error%B error%C error%D%EQ Connection reset peer"
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
            "File nodepool/ %ID/ config_validator.py line %ID validate"
        );
        assert_eq!(
            process("controller |             \"after\": \"3}QP5CJuNBP65S%c:y>o\"",),
            "controller after%EQ %ODD"
        );
        assert_eq!(
            process("[Zuul] Job complete, result: FAILURE"),
            "Zuul complete result%EQ FAILURE FAILURE%A FAILURE%B FAILURE%C FAILURE%D"
        );
    }

    #[test]
    fn test_process04() {
        assert_eq!(
            process("\"assertion\": \"new_dhcp is changed\""),
            "assertion assertion%A assertion%B assertion%C assertion%D%EQ new_dhcp changed"
        );
    }

    #[test]
    fn test_process20() {
        assert_eq!(
            process("controller | +3}QP5CJuNBP65S%c:y>o"),
            process("controller | +1T9,Eqb@g[VL@b0u*Et!")
        );
        assert_eq!(
            process("   \"contents\": \"3}QP5CJuNBP65S%c:y>o\""),
            process("   \"contents\": \"U%aNO^b5ITFU^xTTa9rV\",")
        );
        assert_eq!(
            process(
                "pkg: openstack-tripleo-heat-templates-13.5.1-0.20220121152841.1408598.el8.noarch"
            ),
            "%ID %DASH"
        );
        tokens_eq!(
            "id = \"HvXxSk-Foz9-XJE4-RZSD-KXxc-NxTt-AMi18O\"",
            "id = \"BBW6bE-58DO-3GeE-3ix2-8pLG-wfWL-aiTdAf\""
        );
        tokens_eq!(
            "rabbitmq::erlang_cookie: xkkGdfgqlUovQz3fP2CZ",
            "rabbitmq::erlang_cookie: xkkGdfgqlUovQz3fP2CZ"
        );
        tokens_eq!(
            "ZUUL_REF=Z60f0ad207fbb4c55a07d665ef44131a4",
            "ZUUL_REF=Zbffe5ccbe3ef4ab48c016783ea185dfa"
        );
        tokens_eq!("tap44302f40-8", "tap423e2e40-8");
        tokens_eq!(
            "[fd00:fd00:fd00:2000::21e]:5672 (1)",
            "[fd00:ad00:fd00:2100::21e]:5872 (1)"
        );
        tokens_eq!(
            "DHCPREQUEST(tap44302f40-82) 192.168.24.9 fa:16:3e:94:88:3f",
            "DHCPREQUEST(tap443e2140-82) 192.168.25.9 fb:16:3e:94:88:3f"
        );
        tokens_eq!(
            r"\ = Local Signing Authority, CN = caa53b4e-fff041fe-93823ed2-7ee25a11\n\n\",
            r"\ = Local Signing Authority, CN = 41319aee-68934f60-baf41d6e-158a15cd\n\n\"
        );
        tokens_eq!(
            r"Baremetal Node@83d24142-5411-4568-b344-05caac9fcfbf: {}",
            r"Baremetal Node@e54437f7-1f1d-4a9b-8cc5-ce73550f8608: {}"
        );
    }

    #[test]
    fn test_process21() {
        tokens_eq!(
            r"-netdev tap,fd=123,id=hostnet0 \",
            r"-netdev tap,fd=175,id=hostnet0 \"
        );
        tokens_eq!(
            r"-device virtio-net-pci,rx_queue_size=512,host_mtu=1292,netdev=hostnet0,id=net0,mac=fa:16:3e:a3:dc:e1,bus=pci.0,addr=0x3",
            r"-device virtio-net-pci,rx_queue_size=52,host_mtu=12920,netdev=hostnet0,id=net0,mac=fa:16:3e:1a:1c:fd,bus=pci.1,addr=0x4"
        );
    }

    #[test]
    fn test_process22() {
        tokens_eq!(
            "creating Value \"ApacheNetworks\" Stack \"undercloud-UndercloudServiceChain-sczoll7kpg37-ServiceChain-ghee7usnfx3j-17-wztq7dmj6blw-ApacheServiceBase-7nwdrcrxjpmz",
            "creating Value \"ApacheNetworks\" Stack \"undercloud-UndercloudServiceChain-dt26w6s63vd6-ServiceChain-dxxxgncfjqeg-0-yhtbooauehxj"
        );
    }

    #[test]
    fn test_process23() {
        assert_eq!(
            process("  mysql::server::root_password: Lj3glPogKC"),
            "mysql%EQ :server::root_password:"
        );
        assert_eq!(
            process("content: eIjsbTkEe8xGeThoRhNUaO-UbzrGdQ5CQpX38rjNLVw="),
            "content%EQ %BASE64"
        );
    }

    #[test]
    fn test_process24() {
        assert_eq!(
            process("Jul 30 21:51:01 localhost elastic_mirzakhani[36129]: 167 167"),
            "%ID %ID localhost %UNIT %ID %ID"
        );
    }

    #[test]
    fn test_process_ovn() {
        assert_eq!(
            process("addresses: [\"fa:16:3e:69:3c:cd\"]"),
            "addresses%EQ %ID"
        );
        assert_eq!(
            process("addresses: [\"fa:16:3e:19:15:bb 192.168.199.2\"]"),
            "addresses%EQ %ID %ID"
        );
    }

    #[test]
    fn test_process_amqp() {
        assert_eq!(
            process("closing AMQP connection <0.4375.0> ([fd00:fd00:fd00:2000::40]:33588 -> [fd00:fd00:fd00:2000::21e]:5672 - nova-compute:8:08b39730-b2e6-4d1f-bcc1-318f9bcfd7c6, vhost: '/', user: 'guest')"),
            "closing AMQP connection %ID %ID %ID nova- compute%EQ %ID vhost%EQ user%EQ guest"
        );
    }

    #[test]
    fn test_kv() {
        assert_eq!(
            process("a name=delorean-tripleo-repos-8c402732195f680e7bf8197030cb5a25d45df5a9"),
            "%ID name%EQ delorean- tripleo- repos- %ID"
        );
    }

    #[test]
    fn test_words() {
        assert_eq!(
            words(" a b ").collect::<Vec<&str>>(),
            vec!["", "a", "b", ""]
        );
    }

    #[test]
    fn test_space_separated_kv() {
        assert_eq!(
            process("Token: roAkIx7BqBtdjHW42TdRcwpN6fdCI4Weym7-PibmF7o"),
            "Token%EQ %VALUE_ID"
        )
    }

    #[test]
    fn test_pipeline_name() {
        assert_eq!(
            process("2023-09-22 18:15:00.229959 | Pipeline: check"),
            "%ID %ID Pipeline%EQ %VALUE_ID"
        )
    }

    #[test]
    fn test_consonant() {
        assert_eq!(process("Name: install-pb96q"), "Name%EQ install- %ID")
    }

    #[test]
    fn test_consonant2() {
        assert_eq!(
            process("ZooKeeper /nodepool/components/launcher/nodepool-launcher-fbb79bd59-f8dvh"),
            process("ZooKeeper /nodepool/components/launcher/nodepool-launcher-8644d87556-kdlfj"),
        )
    }
    #[test]
    fn test_consonant3() {
        assert_eq!(
            process("Name: logserver-6cc7669744-bf2b2"),
            process("Name: logserver-7d748d77c-9xgn2"),
        );
        assert_eq!(
            process("Name: logserver-6cc7669744-bf2b2"),
            "Name%EQ logserver- %ID",
        );
    }

    #[test]
    fn test_comma() {
        assert_eq!(
            process("Endpoints: 10.42.0.51:7900,10.42.0.52:7900"),
            process("Endpoints: 10.42.0.40:7900"),
        )
    }
}
