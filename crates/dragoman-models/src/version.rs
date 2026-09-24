// SPDX-License-Identifier: GPL-3.0-or-later

//! Mozilla toolkit version ordering, for the version shapes that occur in
//! the translations collections: `3.0`, `3.0a`, `3.0a1`, `3.1`, ….
//! Ordering: `3.0a < 3.0a1 < 3.0a2 < 3.0 < 3.1` (an absent letter part
//! sorts *after* a present one). See docs/model-compatibility.md.

use std::cmp::Ordering;
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MozVersion {
    raw: String,
    parts: Vec<Part>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Part {
    number: u64,
    /// `true` when there is no letter suffix; sorts after any suffix.
    is_final: bool,
    alpha: String,
    pre_number: u64,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
#[error("unsupported version syntax: {0:?}")]
pub struct ParseError(String);

impl MozVersion {
    pub fn parse(raw: &str) -> Result<Self, ParseError> {
        let mut parts = Vec::new();
        for piece in raw.split('.') {
            let digits: String = piece.chars().take_while(char::is_ascii_digit).collect();
            let rest = &piece[digits.len()..];
            let alpha: String = rest.chars().take_while(char::is_ascii_alphabetic).collect();
            let pre = &rest[alpha.len()..];
            if digits.is_empty() || !pre.chars().all(|c| c.is_ascii_digit()) {
                return Err(ParseError(raw.to_owned()));
            }
            parts.push(Part {
                number: digits.parse().map_err(|_| ParseError(raw.to_owned()))?,
                is_final: alpha.is_empty(),
                alpha,
                pre_number: if pre.is_empty() {
                    0
                } else {
                    pre.parse().map_err(|_| ParseError(raw.to_owned()))?
                },
            });
        }
        if parts.is_empty() {
            return Err(ParseError(raw.to_owned()));
        }
        Ok(MozVersion {
            raw: raw.to_owned(),
            parts,
        })
    }

    /// The leading number: `3` for `3.0a1`.
    pub fn major(&self) -> u64 {
        self.parts[0].number
    }

    pub fn as_str(&self) -> &str {
        &self.raw
    }
}

impl Ord for MozVersion {
    fn cmp(&self, other: &Self) -> Ordering {
        // Missing trailing parts count as zero-final ("3" == "3.0").
        let zero = Part {
            number: 0,
            is_final: true,
            alpha: String::new(),
            pre_number: 0,
        };
        let len = self.parts.len().max(other.parts.len());
        for i in 0..len {
            let a = self.parts.get(i).unwrap_or(&zero);
            let b = other.parts.get(i).unwrap_or(&zero);
            let ord = a.cmp(b);
            if ord != Ordering::Equal {
                return ord;
            }
        }
        Ordering::Equal
    }
}

impl PartialOrd for MozVersion {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl fmt::Display for MozVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.raw)
    }
}

#[cfg(test)]
mod tests {
    use super::MozVersion;

    fn v(s: &str) -> MozVersion {
        MozVersion::parse(s).unwrap()
    }

    #[test]
    fn ordering_matches_mozilla() {
        // The shapes that occur live, in ascending order.
        let ascending = [
            "1.0a", "1.0a1", "1.0a2", "1.0", "1.1a1", "1.1", "2.0", "3.0a1", "3.0", "3.1",
        ];
        for pair in ascending.windows(2) {
            assert!(
                v(pair[0]) < v(pair[1]),
                "{} should sort before {}",
                pair[0],
                pair[1]
            );
        }
    }

    #[test]
    fn equality_and_padding() {
        assert_eq!(v("3.0"), v("3.0"));
        assert_eq!(v("3").cmp(&v("3.0")), std::cmp::Ordering::Equal);
        assert!(v("3.0a") < v("3"));
    }

    #[test]
    fn major_and_window() {
        assert_eq!(v("3.0a1").major(), 3);
        assert_eq!(v("2.3").major(), 2);
    }

    #[test]
    fn rejects_garbage() {
        for bad in ["", "a", "3.0-beta", "3..0", "3.0a1b2"] {
            assert!(MozVersion::parse(bad).is_err(), "{bad:?} should not parse");
        }
    }
}
