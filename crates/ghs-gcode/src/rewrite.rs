//! In-place text edits shared by the source-to-source transforms.
//!
//! A transform rewrites only the words it changes and copies every other byte
//! of the line. These helpers locate a word's numeric token and splice
//! replacements without ever indexing outside the line.

/// One in-place edit of a source line.
#[derive(Debug, Clone)]
pub(crate) struct Edit {
    /// Byte offset of the first replaced byte, or the insertion point.
    pub(crate) start: usize,
    /// Byte offset one past the last replaced byte; equals `start` to insert.
    pub(crate) end: usize,
    /// Replacement text.
    pub(crate) text: String,
}

impl Edit {
    /// Creates a replacement for a byte range.
    pub(crate) fn replace(start: usize, end: usize, text: String) -> Self {
        Self { start, end, text }
    }

    /// Creates an insertion at a byte offset.
    pub(crate) fn insert(at: usize, text: String) -> Self {
        Self {
            start: at,
            end: at,
            text,
        }
    }
}

/// Applies non-overlapping edits to `source`, preserving everything else.
///
/// Returns `None` when an edit is out of bounds or overlaps an earlier one, so
/// a bad span can never panic or corrupt the line.
pub(crate) fn apply_edits(source: &str, mut edits: Vec<Edit>) -> Option<String> {
    edits.sort_by(|a, b| a.start.cmp(&b.start).then(a.end.cmp(&b.end)));
    let mut output = String::with_capacity(source.len() + 32);
    let mut cursor = 0usize;
    for edit in edits {
        if edit.start < cursor || edit.end < edit.start || edit.end > source.len() {
            return None;
        }
        output.push_str(source.get(cursor..edit.start)?);
        output.push_str(&edit.text);
        cursor = edit.end;
    }
    output.push_str(source.get(cursor..)?);
    Some(output)
}

/// Returns the byte range of the numeric token of a word.
///
/// `column` is the 1-based byte column of the letter, as reported by the lexer.
/// The returned range starts after the letter and excludes it.
pub(crate) fn number_span(source: &str, column: usize) -> Option<(usize, usize)> {
    let letter = column.checked_sub(1)?;
    let bytes = source.as_bytes();
    if !bytes.get(letter)?.is_ascii_alphabetic() {
        return None;
    }
    let start = letter + 1;
    let mut end = start;
    while let Some(byte) = bytes.get(end) {
        if matches!(byte, b'0'..=b'9' | b'+' | b'-' | b'.') {
            end += 1;
        } else {
            break;
        }
    }
    (end > start).then_some((start, end))
}

/// Returns the byte range of a complete word, including its letter.
pub(crate) fn word_span(source: &str, column: usize) -> Option<(usize, usize)> {
    let (start, end) = number_span(source, column)?;
    Some((start.saturating_sub(1), end))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numeric_spans_cover_the_whole_token() {
        assert_eq!(number_span("G0 X-1.25", 4), Some((4, 9)));
        assert_eq!(word_span("G0 X-1.25", 4), Some((3, 9)));
        assert_eq!(number_span("g1x10", 3), Some((3, 5)));
        assert_eq!(number_span("G0 X", 4), None);
        assert_eq!(number_span("G0 X1", 2), None);
        assert_eq!(number_span("", 1), None);
        assert_eq!(number_span("G0 X1", 0), None);
    }

    #[test]
    fn edits_preserve_text_outside_their_spans() {
        let mut edits = vec![
            Edit::replace(4, 5, "9".to_string()),
            Edit::insert(5, " Y2".to_string()),
        ];
        assert_eq!(
            apply_edits("G1 X1Z0", edits.clone()).as_deref(),
            Some("G1 X9 Y2Z0")
        );
        edits.push(Edit::replace(8, 7, "bad".to_string()));
        assert_eq!(apply_edits("G1 X1Z0", edits), None);
    }
}
