//! Minimal G-code word parsing for the simulator.
//!
//! This is deliberately not the full `ghs-gcode` parser: the simulator only
//! needs the modal words it reports and executes. Parsing is bounded and
//! allocation-light, and malformed input returns an error instead of panicking.

/// Maximum words accepted on one line.
const MAX_WORDS: usize = 64;

/// One parsed G-code word.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Word {
    /// Upper-case word letter.
    pub letter: char,
    /// Numeric value; `0.0` when the word has no number.
    pub value: f64,
    /// Whether a number followed the letter.
    pub has_value: bool,
}

impl Word {
    /// Returns `true` when the word is one of the nine axis letters.
    #[must_use]
    pub(crate) fn is_axis(self) -> bool {
        matches!(
            self.letter,
            'X' | 'Y' | 'Z' | 'A' | 'B' | 'C' | 'U' | 'V' | 'W'
        )
    }
}

/// Parse a G-code line into words.
///
/// Comments (`(...)` and `; ...`) are removed; an optional trailing `*`
/// checksum is ignored. A leading `/` block-delete marker is skipped. The
/// function never panics and rejects malformed or oversized input.
///
/// # Errors
///
/// Returns `Err(())` when a number cannot be parsed, a stray character is
/// present, or the line carries more than [`MAX_WORDS`] words.
pub(crate) fn parse_line(text: &str) -> Result<Vec<Word>, ()> {
    let mut words = Vec::new();
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        let ch = ch.to_ascii_uppercase();
        if ch.is_whitespace() || ch == '/' {
            continue;
        }
        if ch == ';' {
            break;
        }
        if ch == '(' {
            for comment in chars.by_ref() {
                if comment == ')' {
                    break;
                }
            }
            continue;
        }
        if ch == '*' {
            break;
        }
        if !ch.is_ascii_alphabetic() {
            return Err(());
        }
        let mut number = String::new();
        while let Some(next) = chars.peek().copied() {
            if next.is_ascii_digit() || next == '.' || next == '-' || next == '+' {
                number.push(next);
                chars.next();
            } else {
                break;
            }
        }
        let has_value = !number.is_empty();
        let value = if has_value {
            let parsed = number.parse::<f64>().map_err(|_| ())?;
            if !parsed.is_finite() {
                return Err(());
            }
            parsed
        } else {
            0.0
        };
        if words.len() >= MAX_WORDS {
            return Err(());
        }
        words.push(Word {
            letter: ch,
            value,
            has_value,
        });
    }
    Ok(words)
}

/// Compare a parsed code value with a literal G- or M-code.
#[must_use]
pub(crate) fn is_code(value: f64, code: f64) -> bool {
    (value - code).abs() < 1e-6
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_words_and_ignores_comments() {
        let words = parse_line("G1 X10.5 Y-2 (move) ; trailing").expect("valid");
        assert_eq!(words.len(), 3);
        assert_eq!(words[0].letter, 'G');
        assert_eq!(words[0].value, 1.0);
        assert_eq!(words[1].letter, 'X');
        assert_eq!(words[1].value, 10.5);
        assert_eq!(words[2].letter, 'Y');
        assert_eq!(words[2].value, -2.0);
    }

    #[test]
    fn lower_case_and_block_delete_are_accepted() {
        let words = parse_line("/g0x1y2").expect("valid");
        assert_eq!(words[0].letter, 'G');
        assert_eq!(words[0].value, 0.0);
        assert_eq!(words[1].letter, 'X');
        assert_eq!(words[2].letter, 'Y');
    }

    #[test]
    fn checksum_suffix_is_ignored() {
        let words = parse_line("G1 X1*42").expect("valid");
        assert_eq!(words.len(), 2);
    }

    #[test]
    fn malformed_input_is_rejected() {
        assert!(parse_line("G1 X").is_ok());
        assert!(parse_line("X1 Y").is_ok());
        assert!(parse_line("!G1").is_err());
        assert!(parse_line("G1 X1..2").is_err());
        let long = "G1 ".to_string() + &"X1 ".repeat(100);
        assert!(parse_line(&long).is_err());
    }

    #[test]
    fn words_without_values_are_marked() {
        let words = parse_line("G1 X").expect("valid");
        assert!(words[1].letter == 'X' && !words[1].has_value);
        assert!(words[1].is_axis());
    }
}
