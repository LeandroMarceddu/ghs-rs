//! Bounded, no-motion operator macros around a probe program.
//!
//! A pre or post macro is operator text inserted before the first and after
//! the last probe move. The allowlist is deliberately narrow so a typo can
//! never add motion, probing, spindle or coolant start, a tool change, a
//! coordinate change, or a system command:
//!
//! - blank lines are dropped and comments (`(...)` or `;...`) are preserved;
//! - `G4 P<seconds>` with exactly one `P` word and `0 <= P <= 60`;
//! - `M0`, `M1`, `M5`, and `M9`, case-insensitive.
//!
//! Every allowed command may carry one trailing comment. Two commands on one
//! line, nested comments, and any other word or command are rejected.

use crate::error::ProbeMacroError;

/// Maximum byte length of one macro, measured before trimming.
pub const MAX_PROBE_MACRO_BYTES: usize = 512;

/// Maximum number of cleaned, non-blank lines in one macro.
pub const MAX_PROBE_MACRO_LINES: usize = 8;

/// Longest accepted `G4` dwell, in seconds. The range starts at zero.
pub const MAX_PROBE_MACRO_DWELL_SECONDS: f64 = 60.0;

/// Validate an operator pre or post probe macro.
///
/// Returns the cleaned program lines: each line is trimmed, blank lines are
/// dropped, and comments and their text are preserved. The result is bounded by
/// [`MAX_PROBE_MACRO_BYTES`] and [`MAX_PROBE_MACRO_LINES`].
///
/// Accepted lines are comments (`(...)` or `;...`), `G4 P<seconds>` with one
/// `P` word and `0 <= P <= 60`, and `M0`, `M1`, `M5`, `M9` in any case. An
/// allowed command may carry one trailing comment. Everything else is rejected
/// with the one-based line number and trimmed text: motion and probing, spindle
/// or coolant start, tool changes, coordinate changes, system commands, a
/// second command on the same line, and control characters other than tab.
///
/// This function is total: any input returns a value or an error and never
/// panics.
pub fn validate_probe_macro(text: &str) -> Result<Vec<String>, ProbeMacroError> {
    if text.len() > MAX_PROBE_MACRO_BYTES {
        return Err(ProbeMacroError::TooManyBytes {
            bytes: text.len(),
            max: MAX_PROBE_MACRO_BYTES,
        });
    }

    let mut cleaned: Vec<(usize, &str)> = Vec::new();
    for (offset, raw) in text.lines().enumerate() {
        let number = offset + 1;
        if let Some(character) = raw.chars().find(|c| c.is_control() && *c != '\t') {
            return Err(ProbeMacroError::ControlCharacter {
                line: number,
                character,
            });
        }
        let line = raw.trim();
        if !line.is_empty() {
            cleaned.push((number, line));
        }
    }
    if cleaned.len() > MAX_PROBE_MACRO_LINES {
        return Err(ProbeMacroError::TooManyLines {
            lines: cleaned.len(),
            max: MAX_PROBE_MACRO_LINES,
        });
    }

    let mut lines = Vec::with_capacity(cleaned.len());
    for (number, line) in cleaned {
        validate_line(number, line)?;
        lines.push(line.to_string());
    }
    Ok(lines)
}

fn validate_line(number: usize, line: &str) -> Result<(), ProbeMacroError> {
    if line.starts_with(';') {
        return Ok(());
    }
    if line.starts_with('(') {
        return if is_comment_only(line) {
            Ok(())
        } else {
            Err(forbidden(number, line))
        };
    }

    let (code, comment) = split_comment(line);
    if let Some(comment) = comment
        && !is_comment_only(comment)
    {
        return Err(forbidden(number, line));
    }
    let words: Vec<&str> = code.split_ascii_whitespace().collect();
    let Some(head) = words.first() else {
        return Err(forbidden(number, line));
    };
    if head.eq_ignore_ascii_case("G4") {
        return validate_dwell(number, line, &words);
    }
    let allowed_m = ["M0", "M1", "M5", "M9"]
        .iter()
        .any(|allowed| head.eq_ignore_ascii_case(allowed));
    if allowed_m && words.len() == 1 {
        return Ok(());
    }
    Err(forbidden(number, line))
}

fn validate_dwell(number: usize, line: &str, words: &[&str]) -> Result<(), ProbeMacroError> {
    if words.len() < 2 {
        return Err(ProbeMacroError::DwellMissingP {
            line: number,
            text: line.to_string(),
        });
    }
    if words.len() > 2 {
        return Err(ProbeMacroError::DwellExtraWords {
            line: number,
            text: line.to_string(),
        });
    }
    let Some(value) = words.get(1).and_then(|word| strip_p_word(word)) else {
        return Err(ProbeMacroError::DwellMissingP {
            line: number,
            text: line.to_string(),
        });
    };
    if value.is_empty() {
        return Err(ProbeMacroError::DwellMissingP {
            line: number,
            text: line.to_string(),
        });
    }
    let Some(seconds) = parse_seconds(value) else {
        return Err(ProbeMacroError::DwellInvalidValue {
            line: number,
            text: line.to_string(),
        });
    };
    if !(0.0..=MAX_PROBE_MACRO_DWELL_SECONDS).contains(&seconds) {
        return Err(ProbeMacroError::DwellOutOfRange {
            line: number,
            text: line.to_string(),
            seconds,
        });
    }
    Ok(())
}

/// The text after a leading `P` or `p`, or `None` for any other word.
fn strip_p_word(word: &str) -> Option<&str> {
    let first = word.as_bytes().first()?;
    if first.eq_ignore_ascii_case(&b'P') {
        word.get(1..)
    } else {
        None
    }
}

/// Parse a plain unsigned or signed decimal; exponents and suffixes are
/// deliberately not accepted.
fn parse_seconds(value: &str) -> Option<f64> {
    let unsigned = value
        .strip_prefix('-')
        .or_else(|| value.strip_prefix('+'))
        .unwrap_or(value);
    let mut dots = 0usize;
    let mut digits = 0usize;
    for byte in unsigned.bytes() {
        match byte {
            b'0'..=b'9' => digits += 1,
            b'.' => {
                dots += 1;
                if dots > 1 {
                    return None;
                }
            }
            _ => return None,
        }
    }
    if digits == 0 {
        return None;
    }
    value
        .parse::<f64>()
        .ok()
        .filter(|seconds| seconds.is_finite())
}

/// Split a line at its first comment start, keeping the comment marker.
fn split_comment(line: &str) -> (&str, Option<&str>) {
    match line.find(['(', ';']) {
        Some(index) => (&line[..index], Some(&line[index..])),
        None => (line, None),
    }
}

/// Whether `text` is comments and whitespace only.
///
/// A `;` consumes the rest of the line. `(...)` comments are flat: a
/// controller that ends a comment at the first `)` must never see command text
/// after a comment on the same line, so nesting is rejected.
fn is_comment_only(text: &str) -> bool {
    let mut depth = 0usize;
    for character in text.chars() {
        match character {
            ';' if depth == 0 => return true,
            '(' if depth == 0 => depth = 1,
            '(' => return false,
            ')' if depth == 1 => depth = 0,
            ')' => return false,
            _ if depth == 1 => {}
            _ if character.is_whitespace() => {}
            _ => return false,
        }
    }
    depth == 0
}

fn forbidden(line: usize, text: &str) -> ProbeMacroError {
    ProbeMacroError::ForbiddenLine {
        line,
        text: text.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(text: &str) -> Vec<String> {
        validate_probe_macro(text).expect("macro should validate")
    }

    #[test]
    fn empty_and_blank_macros_have_no_lines() {
        assert!(lines("").is_empty());
        assert!(lines("   ").is_empty());
        assert!(lines("\n\t\n  \n").is_empty());
    }

    #[test]
    fn allowed_lines_are_accepted() {
        let text = concat!(
            "(pre-probe)\n",
            "; semicolon note\n",
            "G4 P0\n",
            "G4 P60\n",
            "g4 p30.5\n",
            "M0\n",
            "M1\n",
            "M5\n",
        );
        let cleaned = lines(text);
        assert_eq!(cleaned.len(), MAX_PROBE_MACRO_LINES);
        assert_eq!(cleaned[0], "(pre-probe)");
        assert_eq!(cleaned[1], "; semicolon note");
        assert_eq!(cleaned[7], "M5");
        assert_eq!(lines("M9"), vec!["M9"]);
    }

    #[test]
    fn allowed_m_codes_are_case_insensitive() {
        for text in ["m0", "M1", "m5", "M9", "m0 (stop)", "M5 (spindle off)"] {
            assert!(validate_probe_macro(text).is_ok(), "{text}");
        }
        assert_eq!(lines("M5\r\nM9"), vec!["M5", "M9"]);
    }

    #[test]
    fn dwells_at_and_near_the_bounds() {
        assert!(validate_probe_macro("G4 P0").is_ok());
        assert!(validate_probe_macro("G4 P60").is_ok());
        assert!(validate_probe_macro("G4 P0.0").is_ok());
        assert!(validate_probe_macro("G4 P59.999").is_ok());
        assert!(validate_probe_macro("G4\tP1").is_ok());
        assert!(matches!(
            validate_probe_macro("G4 P60.001"),
            Err(ProbeMacroError::DwellOutOfRange { line: 1, .. })
        ));
        assert!(matches!(
            validate_probe_macro("G4 P61"),
            Err(ProbeMacroError::DwellOutOfRange { seconds, .. }) if seconds == 61.0
        ));
        assert!(matches!(
            validate_probe_macro("G4 P-1"),
            Err(ProbeMacroError::DwellOutOfRange { seconds, .. }) if seconds == -1.0
        ));
    }

    #[test]
    fn dwell_without_p_is_rejected() {
        for text in ["G4", "G4 P", "G4 X1", "G4 (dwell)"] {
            assert!(
                matches!(
                    validate_probe_macro(text),
                    Err(ProbeMacroError::DwellMissingP { .. })
                ),
                "{text}"
            );
        }
    }

    #[test]
    fn dwell_with_extra_words_is_rejected() {
        for text in ["G4 P1 P2", "G4 P1 X0", "G4 P1 F100"] {
            assert!(
                matches!(
                    validate_probe_macro(text),
                    Err(ProbeMacroError::DwellExtraWords { .. })
                ),
                "{text}"
            );
        }
    }

    #[test]
    fn dwell_with_an_unreadable_value_is_rejected() {
        for text in ["G4 Pabc", "G4 P1.2.3", "G4 P1e2", "G4 P0x10", "G4 P."] {
            assert!(
                matches!(
                    validate_probe_macro(text),
                    Err(ProbeMacroError::DwellInvalidValue { .. })
                ),
                "{text}"
            );
        }
    }

    #[test]
    fn forbidden_command_families_are_rejected() {
        let forbidden = [
            "G0 X0",
            "G1 X0 F100",
            "G2 X1 Y1 I1 J0",
            "G3 X1 Y1 I1 J0",
            "G38.2 Z-1 F100",
            "G38.3 Z-1",
            "G38.4 Z1",
            "G38.5 Z1",
            "G10 L2 P1 X0",
            "G92 X0",
            "G92.1",
            "G28",
            "G30",
            "G53 G0 Z0",
            "G51 X0 Y0",
            "G43.1 Z0",
            "G43",
            "G49",
            "M3",
            "M3 S1000",
            "M4",
            "M6 T1",
            "M7",
            "M8",
            "S1000",
            "T1",
            "F100",
            "$H",
            "$X",
            "O100",
            "N10",
            "%",
            "G4P1",
            "G04 P1",
            "M05",
            "M5.1",
        ];
        for text in forbidden {
            assert!(
                matches!(
                    validate_probe_macro(text),
                    Err(ProbeMacroError::ForbiddenLine { line: 1, .. })
                ),
                "{text}"
            );
        }
    }

    #[test]
    fn checksums_and_multiple_commands_are_rejected() {
        for text in ["M5 M9", "M0 M1", "M5 G4 P1", "M0 G4 P1", "G4 P1*33"] {
            assert!(validate_probe_macro(text).is_err(), "{text}");
        }
    }

    #[test]
    fn command_text_after_a_comment_is_rejected() {
        for text in [
            "(safe) M3",
            "M5 (off) M3",
            "(a)(b) G0 X0",
            "; note G0 X0\nM5 G0",
        ] {
            assert!(validate_probe_macro(text).is_err(), "{text}");
        }
    }

    #[test]
    fn comments_are_preserved_and_trimmed() {
        let cleaned = lines("  (pre)  \n\t; note\t\nG4 P1  ");
        assert_eq!(cleaned, vec!["(pre)", "; note", "G4 P1"]);
        assert_eq!(
            lines("M5 ; spindle off\nG4 P2 (settle)"),
            vec!["M5 ; spindle off", "G4 P2 (settle)"]
        );
    }

    #[test]
    fn byte_bound_is_enforced() {
        let at_limit = format!("({})", "a".repeat(MAX_PROBE_MACRO_BYTES - 2));
        assert_eq!(at_limit.len(), MAX_PROBE_MACRO_BYTES);
        assert!(validate_probe_macro(&at_limit).is_ok());

        let over = format!("({})", "a".repeat(MAX_PROBE_MACRO_BYTES - 1));
        assert!(matches!(
            validate_probe_macro(&over),
            Err(ProbeMacroError::TooManyBytes {
                bytes,
                max: MAX_PROBE_MACRO_BYTES
            }) if bytes == MAX_PROBE_MACRO_BYTES + 1
        ));
    }

    #[test]
    fn line_bound_is_enforced() {
        let at_limit: String = (1..=MAX_PROBE_MACRO_LINES)
            .map(|index| format!("(line {index})\n"))
            .collect();
        assert_eq!(lines(&at_limit).len(), MAX_PROBE_MACRO_LINES);

        let over = format!("{at_limit}(one more)\n");
        assert!(matches!(
            validate_probe_macro(&over),
            Err(ProbeMacroError::TooManyLines {
                lines,
                max: MAX_PROBE_MACRO_LINES
            }) if lines == MAX_PROBE_MACRO_LINES + 1
        ));
    }

    #[test]
    fn non_ascii_and_control_characters_never_panic() {
        assert!(validate_probe_macro("(pré-probe)").is_ok());
        assert!(validate_probe_macro("M5 (arrêt)").is_ok());
        for text in [
            "M5\u{0}",
            "M5\u{7}",
            "\u{1b}",
            "(a\u{0}b)",
            "G4 P1\rM0",
            "M\u{301}5",
        ] {
            assert!(validate_probe_macro(text).is_err(), "{text:?}");
        }
        assert!(matches!(
            validate_probe_macro("(a\u{0}b)"),
            Err(ProbeMacroError::ControlCharacter { line: 1, .. })
        ));
        // Long non-ASCII input must also be rejected without panicking.
        assert!(validate_probe_macro(&"é".repeat(MAX_PROBE_MACRO_BYTES)).is_err());
    }

    #[test]
    fn errors_report_the_original_line_number() {
        let error = validate_probe_macro("\n\nG4 P1\nG0 X0").expect_err("forbidden line");
        assert_eq!(
            error,
            ProbeMacroError::ForbiddenLine {
                line: 4,
                text: "G0 X0".to_string(),
            }
        );
    }
}
