//! Line lexer for the grblHAL G-code dialect.
//!
//! The lexer is deliberately independent of modal state: it turns one source
//! line into ordered [`Word`]s, comment text, a block-delete flag, and an
//! optional `*` checksum. Modal interpretation lives in the planner.
//!
//! Accepted syntax:
//!
//! - `G`, `M`, axis, and parameter words with optional sign and decimal point.
//! - `N` line numbers, `T`/`S`/`F` words.
//! - `( ... )` and `;` comments.
//! - A leading `/` block-delete marker.
//! - A trailing `*nn` XOR checksum over the bytes before `*`.
//! - A lone `%` program marker.
//!
//! Expressions (`#`, `[...]`) and flow control (`IF`, `WHILE`, ...) are
//! rejected with [`ParseErrorKind::UnsupportedExpression`]; they are a
//! documented follow-up (VIEW-02), not silently accepted.

use crate::error::{ParseError, ParseErrorKind};

/// Safety cap on words per line when the caller does not set one.
pub const DEFAULT_MAX_WORDS: usize = 256;

/// One letter word, for example `G1` or `X-12.5`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Word {
    /// Upper-case word letter, `A` through `Z`.
    pub letter: char,
    /// Numeric value as written in the program.
    pub value: f64,
    /// 1-based byte column of the letter in the source line.
    pub column: usize,
}

impl Word {
    /// Creates a word.
    #[must_use]
    pub const fn new(letter: char, value: f64, column: usize) -> Self {
        Self {
            letter,
            value,
            column,
        }
    }
}

/// The result of lexing one line.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LexedLine {
    /// Words in source order.
    pub words: Vec<Word>,
    /// Comment text without the surrounding delimiters.
    pub comments: Vec<String>,
    /// `true` when the line starts with the `/` block-delete marker.
    pub block_delete: bool,
    /// Parsed `*` checksum, when present and valid.
    pub checksum: Option<u8>,
    /// `true` for a lone `%` program marker line.
    pub percent: bool,
}

impl LexedLine {
    /// Returns `true` when the line has no executable words.
    #[must_use]
    pub fn has_words(&self) -> bool {
        !self.words.is_empty()
    }
}

/// Flow-control keywords that are recognised only to produce a clear deferral
/// message. They are not G-code words.
const CONTROL_KEYWORDS: [&str; 11] = [
    "IF", "WHILE", "GOTO", "DO", "ENDIF", "ENDWHILE", "ELSE", "SUB", "ENDSUB", "CALL", "RETURN",
];

/// Lexes one source line.
///
/// `line` is used only to label errors. `max_words` is a safety bound; values
/// below one are treated as [`DEFAULT_MAX_WORDS`].
///
/// # Errors
///
/// Returns every lexing error found on the line. The line is not partially
/// returned.
pub fn lex_line(line: usize, source: &str, max_words: usize) -> Result<LexedLine, Vec<ParseError>> {
    let max_words = max_words.max(1);
    let mut errors = Vec::new();

    if let Some(keyword) = leading_control_keyword(source) {
        errors.push(ParseError::new(
            line,
            keyword.0,
            ParseErrorKind::UnsupportedExpression,
            format!(
                "{keyword_name} flow control is not supported yet; expressions and flow control are a documented follow-up",
                keyword_name = keyword.1
            ),
        ));
        return Err(errors);
    }

    let mut lexed = LexedLine::default();
    let bytes = source.as_bytes();
    let mut index = 0usize;

    while index < bytes.len() {
        let byte = bytes[index];
        match byte {
            b' ' | b'\t' | b'\r' => index += 1,
            b'/' if lexed.words.is_empty() && !lexed.percent => {
                lexed.block_delete = true;
                index += 1;
            }
            b'(' => match source[index + 1..].find(')') {
                Some(offset) => {
                    let end = index + 1 + offset;
                    lexed.comments.push(source[index + 1..end].to_string());
                    index = end + 1;
                }
                None => {
                    errors.push(ParseError::new(
                        line,
                        index + 1,
                        ParseErrorKind::UnterminatedComment,
                        "unterminated '(' comment",
                    ));
                    break;
                }
            },
            b';' => {
                lexed.comments.push(source[index + 1..].trim().to_string());
                index = bytes.len();
            }
            b'*' => match parse_checksum(&source[index + 1..]) {
                Ok((checksum, consumed)) => {
                    let computed = checksum_xor(&bytes[..index]);
                    if computed != checksum {
                        errors.push(ParseError::new(
                            line,
                            index + 1,
                            ParseErrorKind::ChecksumMismatch,
                            format!(
                                "checksum mismatch: line has {computed}, program says {checksum}"
                            ),
                        ));
                    } else {
                        lexed.checksum = Some(checksum);
                    }
                    index += 1 + consumed;
                }
                Err(message) => {
                    errors.push(ParseError::new(
                        line,
                        index + 1,
                        ParseErrorKind::InvalidChecksum,
                        message,
                    ));
                    index = bytes.len();
                }
            },
            b'%' => {
                if lexed.words.is_empty() && !lexed.percent && is_lone_percent(&source[index..]) {
                    lexed.percent = true;
                    index = bytes.len();
                } else {
                    errors.push(ParseError::new(
                        line,
                        index + 1,
                        ParseErrorKind::UnexpectedCharacter,
                        "'%' must be alone on its line",
                    ));
                    index += 1;
                }
            }
            b'#' | b'[' | b']' => {
                errors.push(ParseError::new(
                    line,
                    index + 1,
                    ParseErrorKind::UnsupportedExpression,
                    "expressions, parameters, and flow control are not supported yet (VIEW-02 follow-up)",
                ));
                index += 1;
            }
            _ if byte.is_ascii_alphabetic() => {
                let letter = char::from(byte).to_ascii_uppercase();
                let column = index + 1;
                index += 1;
                if index >= bytes.len() {
                    errors.push(ParseError::new(
                        line,
                        column,
                        ParseErrorKind::MissingValue,
                        format!("'{letter}' is not followed by a number"),
                    ));
                    break;
                }
                let next = bytes[index];
                if !(next.is_ascii_digit() || next == b'+' || next == b'-' || next == b'.') {
                    if next == b'#' || next == b'[' {
                        errors.push(ParseError::new(
                            line,
                            column,
                            ParseErrorKind::UnsupportedExpression,
                            "parameter expressions are not supported yet (VIEW-02 follow-up)",
                        ));
                    } else {
                        errors.push(ParseError::new(
                            line,
                            column,
                            ParseErrorKind::MissingValue,
                            format!("'{letter}' is not followed by a number"),
                        ));
                    }
                    break;
                }
                let start = index;
                while index < bytes.len() {
                    let candidate = bytes[index];
                    if candidate.is_ascii_digit()
                        || candidate == b'.'
                        || candidate == b'+'
                        || candidate == b'-'
                    {
                        index += 1;
                    } else {
                        break;
                    }
                }
                let token = &source[start..index];
                match parse_number(token) {
                    Some(value) => lexed.words.push(Word::new(letter, value, column)),
                    None => {
                        errors.push(ParseError::new(
                            line,
                            column,
                            ParseErrorKind::InvalidNumber,
                            format!("'{token}' is not a valid decimal number"),
                        ));
                        break;
                    }
                }
                if lexed.words.len() > max_words {
                    errors.push(ParseError::new(
                        line,
                        column,
                        ParseErrorKind::TooManyWords,
                        format!("more than {max_words} words on one line"),
                    ));
                    break;
                }
            }
            _ => {
                let ch = source[index..].chars().next().unwrap_or('\u{fffd}');
                errors.push(ParseError::new(
                    line,
                    index + 1,
                    ParseErrorKind::UnexpectedCharacter,
                    format!("unexpected character {ch:?}"),
                ));
                index += ch.len_utf8();
            }
        }
    }

    if errors.is_empty() {
        Ok(lexed)
    } else {
        Err(errors)
    }
}

/// Parses a value token that has already been restricted to digits, `.`, `+`,
/// and `-`. Returns `None` for repeated signs, multiple decimal points, a bare
/// sign, a bare dot, and non-finite results.
fn parse_number(token: &str) -> Option<f64> {
    let unsigned = token
        .strip_prefix('-')
        .or_else(|| token.strip_prefix('+'))
        .unwrap_or(token);
    if unsigned.is_empty() {
        return None;
    }
    let mut digits = 0usize;
    let mut dots = 0usize;
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
    token.parse::<f64>().ok().filter(|value| value.is_finite())
}

/// Parses one to three decimal digits after `*`.
///
/// Returns the checksum and the number of bytes consumed (including leading
/// and trailing whitespace that belongs to the checksum field).
fn parse_checksum(rest: &str) -> Result<(u8, usize), String> {
    let trimmed_end = rest.trim_end();
    let trimmed = trimmed_end.trim_start();
    if trimmed.is_empty() {
        return Err("'*' is not followed by a checksum".to_string());
    }
    if !trimmed.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err("checksum must be decimal digits only".to_string());
    }
    if trimmed.len() > 3 {
        return Err("checksum has more than three digits".to_string());
    }
    let value: u8 = trimmed
        .parse()
        .map_err(|_| format!("checksum {trimmed} is out of range"))?;
    Ok((value, rest.len()))
}

/// XOR of every byte in `bytes`, matching grbl's line checksum.
#[must_use]
pub fn checksum_xor(bytes: &[u8]) -> u8 {
    bytes.iter().fold(0u8, |sum, byte| sum ^ byte)
}

/// Returns the column and name of a leading flow-control keyword.
fn leading_control_keyword(source: &str) -> Option<(usize, &'static str)> {
    let trimmed = source.trim_start();
    let column = source.len() - trimmed.len() + 1;
    let mut end = 0usize;
    for (index, ch) in trimmed.char_indices() {
        if ch.is_ascii_alphabetic() {
            end = index + ch.len_utf8();
        } else {
            break;
        }
    }
    if end == 0 {
        return None;
    }
    let candidate = trimmed[..end].to_ascii_uppercase();
    let rest = &trimmed[end..];
    let boundary = rest.is_empty()
        || rest.starts_with(char::is_whitespace)
        || rest.starts_with('[')
        || rest.starts_with('(');
    if !boundary {
        return None;
    }
    CONTROL_KEYWORDS
        .iter()
        .find(|keyword| **keyword == candidate)
        .map(|keyword| (column, *keyword))
}

/// Returns `true` when the only non-space, non-comment content is `%`.
fn is_lone_percent(source: &str) -> bool {
    let mut rest = source.trim();
    if let Some(stripped) = rest.strip_prefix('%') {
        rest = stripped;
    } else {
        return false;
    }
    let rest = match rest.find(';') {
        Some(index) => &rest[..index],
        None => rest,
    };
    let mut rest = rest.trim();
    while let Some(open) = rest.find('(') {
        let Some(close) = rest[open..].find(')') else {
            return false;
        };
        rest = rest[open + close + 1..].trim();
    }
    rest.is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn words_and_comments() {
        let lexed = lex_line(1, "N10 G0 X1 (move) ; tail", DEFAULT_MAX_WORDS).expect("lex");
        assert_eq!(lexed.words.len(), 3);
        assert_eq!(lexed.words[0], Word::new('N', 10.0, 1));
        assert_eq!(lexed.words[1], Word::new('G', 0.0, 5));
        assert_eq!(lexed.words[2], Word::new('X', 1.0, 8));
        assert_eq!(lexed.comments, vec!["move", "tail"]);
        assert!(!lexed.block_delete);
    }

    #[test]
    fn lowercase_and_compact_words() {
        let lexed = lex_line(1, "g1x-2.5y.5", DEFAULT_MAX_WORDS).expect("lex");
        assert_eq!(lexed.words[0], Word::new('G', 1.0, 1));
        assert_eq!(lexed.words[1], Word::new('X', -2.5, 3));
        assert_eq!(lexed.words[2], Word::new('Y', 0.5, 8));
    }

    #[test]
    fn block_delete_and_percent() {
        let lexed = lex_line(1, "/G0 X0", DEFAULT_MAX_WORDS).expect("lex");
        assert!(lexed.block_delete);
        assert_eq!(lexed.words.len(), 2);

        let mark = lex_line(2, "%", DEFAULT_MAX_WORDS).expect("lex");
        assert!(mark.percent);
        assert!(mark.words.is_empty());
    }

    #[test]
    fn checksums_round_trip() {
        let body = "G0 X1";
        let sum = checksum_xor(body.as_bytes());
        let line = format!("{body}*{sum:02}");
        let lexed = lex_line(1, &line, DEFAULT_MAX_WORDS).expect("lex");
        assert_eq!(lexed.checksum, Some(sum));

        let bad = lex_line(1, "G0 X1*99", DEFAULT_MAX_WORDS).expect_err("mismatch");
        assert_eq!(bad[0].kind, ParseErrorKind::ChecksumMismatch);
    }

    #[test]
    fn malformed_input_reports_kind() {
        assert_eq!(
            lex_line(1, "G0 X", DEFAULT_MAX_WORDS).expect_err("missing")[0].kind,
            ParseErrorKind::MissingValue
        );
        assert_eq!(
            lex_line(1, "G0 X1.2.3", DEFAULT_MAX_WORDS).expect_err("number")[0].kind,
            ParseErrorKind::InvalidNumber
        );
        assert_eq!(
            lex_line(1, "(open", DEFAULT_MAX_WORDS).expect_err("comment")[0].kind,
            ParseErrorKind::UnterminatedComment
        );
        assert_eq!(
            lex_line(1, "#1=5", DEFAULT_MAX_WORDS).expect_err("expr")[0].kind,
            ParseErrorKind::UnsupportedExpression
        );
        assert_eq!(
            lex_line(1, "IF [#1 GT 2]", DEFAULT_MAX_WORDS).expect_err("flow")[0].kind,
            ParseErrorKind::UnsupportedExpression
        );
        assert_eq!(
            lex_line(1, "G0 X1*", DEFAULT_MAX_WORDS).expect_err("checksum")[0].kind,
            ParseErrorKind::InvalidChecksum
        );
        assert_eq!(
            lex_line(1, "G0 @", DEFAULT_MAX_WORDS).expect_err("char")[0].kind,
            ParseErrorKind::UnexpectedCharacter
        );
    }

    #[test]
    fn word_limit_is_enforced() {
        let line = "G1 X1 Y1 Z1";
        let errors = lex_line(1, line, 2).expect_err("too many");
        assert_eq!(errors[0].kind, ParseErrorKind::TooManyWords);
    }

    #[test]
    fn numbers_reject_non_finite_and_repeated_signs() {
        assert_eq!(parse_number("1.5"), Some(1.5));
        assert_eq!(parse_number("-.5"), Some(-0.5));
        assert_eq!(parse_number("+7."), Some(7.0));
        assert_eq!(parse_number("1e3"), None);
        assert_eq!(parse_number("--1"), None);
        assert_eq!(parse_number("1-2"), None);
        assert_eq!(parse_number("."), None);
        assert_eq!(parse_number("+"), None);
    }
}
