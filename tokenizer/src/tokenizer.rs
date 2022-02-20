// Copyright (C) 2022 Red Hat
// SPDX-License-Identifier: Apache-2.0

#![warn(missing_docs)]
#![allow(clippy::manual_range_contains)]

//! This library provides a tokenizer function for the [logreduce](https://github.com/logreduce/logreduce) project.
//!
//! The goal is to replace varying words with fixed tokens (e.g. `sha256://...` is converted to `%HASH`).
//!
//! The main function is [process]. The output is designed for further feature extraction,
//! for example with a bag of words or hashing vectorizer. It looks like this:
//!
//! ```rust
//! # use logreduce_tokenizer::{process};
//! assert_eq!(process(
//!    "2017-06-24 02:52:17.732 22627 tempest.lib.common.rest_client [req-b932e095-6706-4f5a-bd75-241c407a9d01 ] Request (main): 201 POST https://10.0.1.9/identity/v3/auth/tokens"),
//!    "%ID %ID %ID tempest.lib.common.rest_client %COOKIE Request main%EQ %ID POST %URL")
//! ```
//!
//! Here are some use cases:
//!
//! ```rust
//! # use logreduce_tokenizer::*;
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
//! # use logreduce_tokenizer::{process};
//! assert_eq!(process("{\"key\": true, \"oth\": 1}"), process("{\"oth\": 1, \"key\": true}"));
//! ```

use lazy_static::lazy_static;
use regex::Regex;
use regex::Split;

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
/// ```rust
/// # use logreduce_tokenizer::{process};
/// assert_eq!(process("iptables -N RULES42 -L"), "%GL_FILTER");
/// assert_eq!(process("e2b607f0bb193c9bfed94af532ba1>33 STORED"), "%GL_FILTER");
/// assert_eq!(process("s/5bf8>28 sending key"), "%GL_FILTER");
/// assert_eq!(process("^- srcf-ntp.example.edu 2 9 377 429 -358us[ -358us] +/- 63ms"), "%GL_FILTER");
/// assert_eq!(process("++ echo mswAxrrS1YwyGtIut9Vd"), "%GL_FILTER");
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
            // chrony logs
            r"|(^\^[+*-] [a-z0-9\.>-]{5,} [0-9])",
            // memcached logs
            r"|(^[a-f0-9s/]+>[0-9]+ )",
            // shell debugs
            r"|(^\+\+ echo [^ ]+$)"
        )).unwrap();
    }
    let is_single_word = !line.contains(|c: char| c.is_whitespace());
    is_single_word || RE.is_match(line)
}

/// Replace numbers sequences with `N`.
/// ```rust
/// # use logreduce_tokenizer::*;
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
/// # use logreduce_tokenizer::*;
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
/// ```rust
/// # use logreduce_tokenizer::*;
/// tokens_eq!("A{$@42", "$A%TE");
/// ```
fn contains_odd_char(word: &str) -> bool {
    lazy_static! {
        static ref RE: Regex = Regex::new(r"[<>{}%$,*]").unwrap();
    }
    RE.is_match(word)
}

/// Check if a word only contains hexa and sep char.
/// ```rust
/// # use logreduce_tokenizer::*;
/// tokens_eq!("the_ip is 127.0.0.1", "the_ip is ::1");
/// tokens_eq!("the_mac is aa:bb:cc", "the_mac is 00:11:cc");
/// tokens_eq!("the_num is 0x4243", "the_num is 0x4142");
/// ```
fn is_uid(word: &str) -> bool {
    lazy_static! {
        static ref RE: Regex =
            Regex::new(concat!("^(:*", r"[\[\]0-9a-fA-FxZ]+[:.-]*", ")+$")).unwrap();
    }
    RE.is_match(word)
}

/// 3 x 4letters word separated by -
fn is_uuid(word: &str) -> bool {
    lazy_static! {
        static ref RE: Regex = Regex::new(concat!(
            "^[a-zA-Z0-9].*-[a-zA-Z0-9]{4}-[a-zA-Z0-9]{4}-[a-zA-Z0-9]{4}-"
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

/// ```rust
/// # use logreduce_tokenizer::*;
/// tokens_eq!("MqoplXLA2LPnJKTNMQW5JpGyMLJcLxRDDEejzh6b1im8KV/5TRKDsg7b5FwBJJoN", "fJkzOzsJdqxvhSvDFkUlAP7a/+kOBCYi1Yp1pz0v/mHLi0r1z5xtx3BemXVYHbom");
/// tokens_eq!("a EqTsSXKlOsEjfIdFld+uwopnIIqvKI+Xu6e0RcAGYJEfj56/MG2IdH7/h1JmQ///\\nn2RZ/ocRcL5as2EHQES0b+/I12a2Gj+W+ub0OQAGDq8iL5o8P0/ogEWrpZmoBC+oi",
///            "a MqoplXLA2LPnJKTNMQW5JpGyMLJcLxRDDEejzh6b1im8KV/5TRKDsg7b5FwBJJoN fJkzOzsJdqxvhSvDFkUlAP7a/+kOBCYi1Yp1pz0v/mHLi0r1z5xtx3BemXVYHbom");
/// ```
fn is_base64(word: &str) -> bool {
    lazy_static! {
        static ref RE: Regex = Regex::new(concat!("^", "[A-Za-z0-9+/=]+", "$")).unwrap();
    }
    word.ends_with("==") || (word.len() > 24 && RE.is_match(word))
}

/// ```
/// # use logreduce_tokenizer::*;
/// tokens_eq!("md5:d41d8cd98f00b204e9800998ecf8427e", "md5:e7b26fc34f528b5b19c4450867b9d597")
/// ```
fn is_hash(word: &str) -> bool {
    lazy_static! {
        static ref RE: Regex = Regex::new(concat!("(?i:^", "(hash|sha|md)[0-9]*:", ")")).unwrap();
    }
    RE.is_match(word)
}

fn is_refs(word: &str) -> bool {
    lazy_static! {
        static ref RE: Regex = Regex::new(concat!(r"^\w{7}\.\.\w{7}$")).unwrap();
    }
    word.starts_with("refs/") || word.starts_with("repos/") || RE.is_match(word)
}

/// ```
/// # use logreduce_tokenizer::*;
/// tokens_eq!("key=01:02:ff", "key=aa:bb:cc")
/// ```
// TODO: check for word terminated by `:`, where the value is the next word
fn is_key_value(word: &str) -> Option<(&str, &str)> {
    match word.split_once(|c| c == '=' || c == ':') {
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

/// Separate attached words like `DHCPOFFER(ipaddr)` in `DHCPOFFER ipaddr`
fn is_two_words(word: &str) -> Option<(&str, &str)> {
    word.split_once(|c| matches!(c, '[' | '(' | '\\' | '@'))
        .map(|(k, v)| (k, v.trim_end_matches(|c| c == ']' || c == ')')))
}

fn is_key_for_id(word: &str) -> bool {
    lazy_static! {
        static ref RE: Regex = Regex::new(concat!(
            "(?i:",
            "(id|key|ref|region|token|secret|password)",
            ")"
        ))
        .unwrap();
    }
    RE.is_match(word)
}

fn is_random_path(word: &str) -> bool {
    word.contains("tmp/") || word.contains("/tmp")
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
    } else {
        None
    }
}

fn trim_pid(word: &str) -> Option<&str> {
    word.trim_end_matches(|c| c >= '0' && c <= '9')
        .strip_suffix('[')
}

/// Makes error token appears bigger.
/// ```rust
/// # use logreduce_tokenizer::*;
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
    result.push_str("%D");
}

/// The tokenizer main (recursive) function
fn do_process(mut word: &str, result: &mut String) -> bool {
    word = trim_quote_and_punctuation(word);
    let mut added = true;
    // We try to process from the most specifics to the most general case
    if let Some(token) = parse_literal(word) {
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
            added = do_process(value, result)
        }
    } else if let Some((w1, w2)) = word.split_once('/') {
        if do_process(w1, result) {
            result.push_str("/ ");
        }
        added = do_process(w2, result);
    } else if let Some((w1, w2)) = word.split_once('-') {
        if has_many_dash(w2) {
            // when word contains more than 4 dash, then consider it noise.
            // e.g. heat uid looks like: undercloud-UndercloudServiceChain-dt26w6s63vd6-ServiceChain-dxxxgncfjqeg-0-yhtbooauehxj
            result.push_str("%DASH")
        } else {
            if do_process(w1, result) {
                result.push_str("- ");
            }
            added = do_process(w2, result)
        }
    } else if let Some((w1, w2)) = word.split_once('|') {
        if do_process(w1, result) {
            result.push_str("| ");
        }
        added = do_process(w2, result)
    } else if word.len() >= 32 {
        result.push_str("%BIG")
    } else if let Some((w1, w2)) = is_two_words(word) {
        if do_process(w1, result) {
            result.push(' ');
        }
        added = do_process(w2, result);
    } else {
        // here finally the word is added
        let x = remove_numbers(word);
        if x.len() > 3 {
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
    for word in words(line) {
        if do_process(word, &mut result) {
            result.push(' ')
        }
    }
    // TODO: check if result contains at least 2 word
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
            "%HASH tata finished systemd%PID"
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
            "nova%EQ :placement::password: %BASE64"
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
            "%ID- %ID- NTN:N:N.NZ| %ID| jsonrpc| WARN WARN%A WARN%B WARN%C WARN%D| %EQ %ID receive error error%A error%B error%C error%D%EQ Connection reset peer"
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
            "File nodepool/ config_validator.py line %ID validate"
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
            "%EQ %DASH"
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
            "closing AMQP connection %ID %ID %ID %UID vhost%EQ user%EQ guest"
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
}
