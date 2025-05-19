// Copyright (C) 2022 Red Hat
// SPDX-License-Identifier: Apache-2.0

//! Logfile generator
//!
//! The main function is [gen_lines]:
//!
//! ```rust
//! # use logjuicer_generate::{gen_lines};
//! assert_eq!(gen_lines().next(), Some("J8xbWovSpJUT zox0 igY5l".to_string()))
//! ```

use rand::prelude::*;
use rand_chacha::ChaCha8Rng;

const SEED: u64 = 42;

fn fixed_rng() -> ChaCha8Rng {
    ChaCha8Rng::seed_from_u64(SEED)
}

fn gen_line(rng: &mut impl Rng) -> String {
    let mut result = String::with_capacity(256);
    for _ in 0..rng.random_range(2..10) {
        let word_size = rng.random_range(2..18);
        let word: String = rng
            .sample_iter(&rand::distr::Alphanumeric)
            .take(word_size)
            .map(char::from)
            .collect();
        result.push_str(&word);
        result.push(' ');
    }
    result.pop();
    result
}

struct RandomLine {
    rng: ChaCha8Rng,
}

impl Iterator for RandomLine {
    type Item = String;

    fn next(&mut self) -> Option<Self::Item> {
        Some(gen_line(&mut self.rng))
    }
}

pub fn gen_lines() -> impl Iterator<Item = String> {
    RandomLine { rng: fixed_rng() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gen_line() {
        let mut rng = fixed_rng();
        assert_eq!(gen_line(&mut rng), "J8xbWovSpJUT zox0 igY5l");
    }

    #[test]
    fn test_gen_lines() {
        let lines = gen_lines().skip(1).take(2).collect::<Vec<String>>();
        assert_eq!(lines,
                   vec!["l5uTLkGKMcREJn RspFXCZ L1Vwir rgr3GSM2OCwCs bPEb4Bpex70wC 187ot3 Y30lq9IOC YXwcMIIr5y1 MdkBVnm3wWxC6",
                        "LxTVf OhUNZNqZ Tbdm2nf8N6hY2d8 h3WZx ADGtPm0 gGc"]);
    }
}
