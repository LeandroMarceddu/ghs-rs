//! Line framing for the grblHAL serial protocol.

use std::fmt;

use crate::response::Response;

/// Default maximum length of one controller line, in bytes.
///
/// grblHAL lines are far shorter in practice; this is a hard bound that keeps a
/// noisy or malicious link from growing the parser without limit.
pub const DEFAULT_MAX_LINE_LEN: usize = 4096;

/// One complete line emitted by [`LineParser`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    text: String,
    truncated: bool,
}

impl Line {
    /// The line contents, decoded as lossy UTF-8 and without the terminator.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.text
    }

    /// Consume the line and return its text.
    #[must_use]
    pub fn into_string(self) -> String {
        self.text
    }

    /// Returns `true` when the line exceeded the parser's maximum length and
    /// was cut at the limit.
    #[must_use]
    pub const fn is_truncated(&self) -> bool {
        self.truncated
    }

    /// Length in bytes.
    #[must_use]
    pub fn len(&self) -> usize {
        self.text.len()
    }

    /// Returns `true` for an empty line.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }
}

impl fmt::Display for Line {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.text)
    }
}

/// Incremental byte-oriented line splitter.
///
/// Accepts `\n`, `\r` and `\r\n` terminators, including grblHAL's `\n\r`
/// sequence. Partial lines are buffered across [`LineParser::push`] calls.
/// Empty lines are skipped. Bytes are decoded with
/// [`String::from_utf8_lossy`], so malformed UTF-8 can never panic.
///
/// The internal buffer is bounded by the configured maximum line length: an
/// overlong line is truncated (marked with [`Line::is_truncated`]) and the rest
/// is discarded up to the next terminator.
#[derive(Debug, Clone)]
pub struct LineParser {
    buffer: Vec<u8>,
    max_line_len: usize,
    truncated: bool,
    skip_lf: bool,
}

impl Default for LineParser {
    fn default() -> Self {
        LineParser::new()
    }
}

impl LineParser {
    /// Create a parser with [`DEFAULT_MAX_LINE_LEN`].
    #[must_use]
    pub fn new() -> Self {
        LineParser::with_max_line_len(DEFAULT_MAX_LINE_LEN)
    }

    /// Create a parser with an explicit maximum line length.
    ///
    /// Values below one are clamped to one so the parser always makes progress.
    #[must_use]
    pub fn with_max_line_len(max_line_len: usize) -> Self {
        LineParser {
            buffer: Vec::new(),
            max_line_len: max_line_len.max(1),
            truncated: false,
            skip_lf: false,
        }
    }

    /// Maximum bytes kept for one line, excluding the terminator.
    #[must_use]
    pub const fn max_line_len(&self) -> usize {
        self.max_line_len
    }

    /// Returns `true` when a partial line is buffered.
    #[must_use]
    pub fn is_partial(&self) -> bool {
        !self.buffer.is_empty() || self.truncated
    }

    /// Bytes of the partial line buffered so far.
    #[must_use]
    pub fn pending(&self) -> &[u8] {
        &self.buffer
    }

    /// Drop any partial line and reset terminator tracking.
    pub fn reset(&mut self) {
        self.buffer.clear();
        self.truncated = false;
        self.skip_lf = false;
    }

    /// Push a chunk of bytes and return every completed line.
    #[must_use]
    pub fn push(&mut self, bytes: &[u8]) -> Vec<Line> {
        let mut lines = Vec::new();
        for &byte in bytes {
            match byte {
                b'\n' => {
                    if self.skip_lf {
                        self.skip_lf = false;
                    } else {
                        self.finish_line(&mut lines);
                    }
                }
                b'\r' => {
                    self.finish_line(&mut lines);
                    self.skip_lf = true;
                }
                _ => {
                    self.skip_lf = false;
                    if self.buffer.len() < self.max_line_len {
                        self.buffer.push(byte);
                    } else {
                        self.truncated = true;
                    }
                }
            }
        }
        lines
    }

    /// Convenience wrapper for pushing text.
    #[must_use]
    pub fn push_str(&mut self, text: &str) -> Vec<Line> {
        self.push(text.as_bytes())
    }

    fn finish_line(&mut self, out: &mut Vec<Line>) {
        self.skip_lf = false;
        let truncated = self.truncated;
        self.truncated = false;

        if self.buffer.is_empty() && !truncated {
            return;
        }

        let text = String::from_utf8_lossy(&self.buffer).into_owned();
        self.buffer.clear();
        out.push(Line { text, truncated });
    }
}

/// Line parser plus response decoding.
///
/// This is the entry point the session uses: feed it raw serial bytes and get
/// typed [`Response`] values back.
#[derive(Debug, Clone, Default)]
pub struct ResponseParser {
    lines: LineParser,
}

impl ResponseParser {
    /// Create a parser with the default maximum line length.
    #[must_use]
    pub fn new() -> Self {
        ResponseParser::default()
    }

    /// Create a parser with an explicit maximum line length.
    #[must_use]
    pub fn with_max_line_len(max_line_len: usize) -> Self {
        ResponseParser {
            lines: LineParser::with_max_line_len(max_line_len),
        }
    }

    /// Access the underlying line parser.
    #[must_use]
    pub fn line_parser(&self) -> &LineParser {
        &self.lines
    }

    /// Returns `true` when a partial line is buffered.
    #[must_use]
    pub fn is_partial(&self) -> bool {
        self.lines.is_partial()
    }

    /// Drop any partial input.
    pub fn reset(&mut self) {
        self.lines.reset();
    }

    /// Push bytes and decode every completed line.
    ///
    /// Overlong lines are reported as [`Response::Unknown`] after a warning, so
    /// they can never be mistaken for a valid controller reply.
    #[must_use]
    pub fn push(&mut self, bytes: &[u8]) -> Vec<Response> {
        self.lines
            .push(bytes)
            .into_iter()
            .map(|line| {
                if line.is_truncated() {
                    tracing::warn!(
                        len = line.len(),
                        "controller line exceeded the maximum length; ignoring it"
                    );
                    Response::Unknown(line.into_string())
                } else {
                    Response::parse(line.as_str())
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(lines: Vec<Line>) -> Vec<String> {
        lines.into_iter().map(Line::into_string).collect()
    }

    #[test]
    fn splits_on_lf_cr_and_crlf() {
        let mut parser = LineParser::new();
        assert_eq!(texts(parser.push(b"ok\n"))[0], "ok");
        assert_eq!(texts(parser.push(b"ok\r"))[0], "ok");
        assert_eq!(texts(parser.push(b"ok\r\n"))[0], "ok");
        assert_eq!(texts(parser.push(b"ok\n\r"))[0], "ok");
        assert!(parser.push(b"\n").is_empty());
        assert!(parser.push(b"\r").is_empty());
    }

    #[test]
    fn buffers_partial_lines_across_pushes() {
        let mut parser = LineParser::new();
        assert!(parser.push(b"<Idle|MPos:0,0").is_empty());
        assert!(parser.is_partial());
        assert_eq!(parser.pending(), b"<Idle|MPos:0,0");
        let lines = parser.push(b",0>\r\n");
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].as_str(), "<Idle|MPos:0,0,0>");
        assert!(!parser.is_partial());
    }

    #[test]
    fn handles_mixed_and_blank_lines() {
        let mut parser = LineParser::new();
        let lines = parser.push(b"one\r\ntwo\n\rthree\n\n\nfour\r");
        assert_eq!(texts(lines), vec!["one", "two", "three", "four"]);
    }

    #[test]
    fn lossy_utf8_never_panics() {
        let mut parser = LineParser::new();
        let lines = parser.push(b"bad \xFF\xFE bytes\r\n");
        assert_eq!(lines.len(), 1);
        assert!(lines[0].as_str().starts_with("bad "));
        assert!(!lines[0].is_truncated());
    }

    #[test]
    fn overlong_lines_are_truncated_and_flagged() {
        let mut parser = LineParser::with_max_line_len(8);
        let lines = parser.push(b"0123456789abcdef\r\nnext\r\n");
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].as_str(), "01234567");
        assert!(lines[0].is_truncated());
        assert_eq!(lines[1].as_str(), "next");
        assert!(!lines[1].is_truncated());
        assert_eq!(parser.push(b"0123456789\r\n")[0].as_str(), "01234567");
    }

    #[test]
    fn zero_max_length_is_clamped() {
        let mut parser = LineParser::with_max_line_len(0);
        assert_eq!(parser.max_line_len(), 1);
        let lines = parser.push(b"ab\r\n");
        assert_eq!(lines[0].as_str(), "a");
        assert!(lines[0].is_truncated());
    }

    #[test]
    fn reset_drops_partial_input() {
        let mut parser = LineParser::new();
        let _ = parser.push(b"partial");
        assert!(parser.is_partial());
        parser.reset();
        assert!(!parser.is_partial());
        assert!(parser.push(b"\r\n").is_empty());
    }

    #[test]
    fn response_parser_decodes_lines() {
        let mut parser = ResponseParser::new();
        assert!(parser.push(b"<Idle|MPos:1.000,2.000,3.000>").is_empty());
        let responses = parser.push(b"\r\nok\r\n");
        assert_eq!(responses.len(), 2);
        assert!(matches!(responses[0], Response::Status(_)));
        assert_eq!(responses[1], Response::Ok);
    }

    #[test]
    fn response_parser_marks_truncated_lines_unknown() {
        let mut parser = ResponseParser::with_max_line_len(4);
        let responses = parser.push(b"averylongline\r\nok\r\n");
        assert_eq!(responses.len(), 2);
        assert!(matches!(&responses[0], Response::Unknown(text) if text == "aver"));
        assert_eq!(responses[1], Response::Ok);
    }
}
