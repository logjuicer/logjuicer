// Copyright (C) 2022 Red Hat
// SPDX-License-Identifier: Apache-2.0

//! Logfile generator
//!
//! The main function is [gen_lines]:
//!
//! ```rust
//! # use logreduce_generate::{gen_lines};
//! assert_eq!(gen_lines().next(), Some("xbWovSpJUTKzox0Pi 5l9xl5uT cREJn spFXCZ wirsrgr 2OCwC pe".to_string()))
//! ```

use rand::distributions::Alphanumeric;
use rand::prelude::*;
use rand_chacha::ChaCha8Rng;

const SEED: u64 = 42;

fn fixed_rng() -> ChaCha8Rng {
    ChaCha8Rng::seed_from_u64(SEED)
}

fn gen_line(rng: &mut impl Rng) -> String {
    let mut result = String::with_capacity(256);
    for _ in 0..rng.gen_range(2..10) {
        let word_size = rng.gen_range(2..18);
        let word: String = rng
            .sample_iter(&Alphanumeric)
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
        assert_eq!(
            gen_line(&mut rng),
            "xbWovSpJUTKzox0Pi 5l9xl5uT cREJn spFXCZ wirsrgr 2OCwC pe"
        );
    }

    #[test]
    fn test_gen_lines() {
        let lines = gen_lines().skip(1).take(2).collect::<Vec<String>>();
        assert_eq!(lines,
                   vec!["wCS187ot3cY30lq OCnY cMIIr5y1tMdkBV WxC6hOLxTVfYOh ZNqZ2 N6hY2d8Mh3WZxUADG GgGcXLI3Ht3CRn1 9eTANY7R5",
                        "cl4 h94 0a bNh3 uOqtbWRmN9P6SocL ni2kJtZU MdUmp mbwE2YmZYnb FI0M5h6RhxeBImoUl"]);
    }
}
