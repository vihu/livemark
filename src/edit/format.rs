//! Formatting keys (REFERENCE-001 section 12): bold, italic and inline code
//! toggle on the selection, the word around the caret, or a new pair of
//! markers; Ctrl/Cmd+K makes a link. Each is one undo step. Whether the
//! caret is inside bold, italic or code comes from the parsed constructs,
//! not from the characters around it: an italic toggle inside `**bold**`
//! must not take one `*` from each side.
use std::time::Duration;

use super::word_at;
use crate::doc::{Change, Doc, Kind, Selection};
use crate::style::{Styled, Syntax};

/// An inline format with a toggle key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    /// `**bold**`, Ctrl/Cmd+B.
    Bold,
    /// `*italic*`, Ctrl/Cmd+I.
    Italic,
    /// `` `code` ``, Ctrl/Cmd+E (the user's pick, 2026-10-02).
    Code,
}

impl Format {
    fn syntax(self) -> Syntax {
        match self {
            Format::Bold => Syntax::Strong,
            Format::Italic => Syntax::Emphasis,
            Format::Code => Syntax::Code,
        }
    }
}

/// Toggles `format` (REFERENCE-001 section 12): off when the selection or
/// caret is inside a construct of that kind; else on the selection, on the
/// word the caret is inside, or as an empty pair around the caret (or a
/// step over the closing marker the caret is right before).
pub fn toggle(doc: &mut Doc, styled: &Styled, format: Format, now: Duration) {
    let selection = doc.selection();
    let range = selection.range();
    let syntax = format.syntax();
    let inside = styled
        .constructs()
        .iter()
        .filter(|c| c.syntax == syntax && c.range.start <= range.start && range.end <= c.range.end)
        .min_by_key(|c| c.range.len());
    if let Some(construct) = inside {
        let [open, close] = construct.markers.clone();
        if range.is_empty() && range.start == close.start && range.start > open.end {
            // Right before the closing marker: step over it.
            doc.set_selection(Selection::caret(close.end));
            return;
        }
        let map = |at: usize| {
            if at <= open.start {
                at
            } else if at <= open.end {
                open.start
            } else if at <= close.start {
                at - open.len()
            } else {
                close.start - open.len()
            }
        };
        let selection = Selection {
            anchor: map(selection.anchor),
            head: map(selection.head),
        };
        doc.apply(
            vec![Change::delete(open), Change::delete(close)],
            selection,
            Kind::Other,
            now,
        );
        return;
    }
    let text = doc.text();
    // A selection is wrapped line by line, each without its spaces at
    // either end and its block markup (`- `, `> `, `# `): `**- a\n**` would
    // break the item and draw the stars.
    let segments = if range.is_empty() {
        vec![strictly_in_word(doc, range.start).unwrap_or(range.clone())]
    } else {
        Some(content_lines(text, range.clone()))
            .filter(|lines| !lines.is_empty())
            .unwrap_or_else(|| vec![range.clone()])
    };
    let marker = match format {
        Format::Bold => "**".to_owned(),
        Format::Italic => "*".to_owned(),
        // A code span's fence is longer than any backtick run inside it.
        Format::Code => {
            let longest = segments
                .iter()
                .flat_map(|s| text[s.clone()].split(|c| c != '`'))
                .map(str::len)
                .max()
                .unwrap_or(0);
            "`".repeat(longest + 1)
        }
    };
    let width = marker.len();
    // Past every opening marker at or before it and every closing one
    // before it: a selection keeps to the text between the markers.
    let moved = |at: usize| {
        let opens = segments.iter().filter(|s| s.start <= at).count();
        let closes = segments.iter().filter(|s| s.end < at).count();
        at + width * (opens + closes)
    };
    let selection = if range.is_empty() {
        Selection::caret(range.start + width)
    } else {
        Selection {
            anchor: moved(selection.anchor),
            head: moved(selection.head),
        }
    };
    let changes = segments
        .iter()
        .flat_map(|s| {
            [
                Change::insert(s.start, marker.clone()),
                Change::insert(s.end, marker.clone()),
            ]
        })
        .collect();
    doc.apply(changes, selection, Kind::Other, now);
}

/// The lines `range` covers, each cut to `range`, without its block
/// markup and without spaces at either end; empty ones left out.
fn content_lines(text: &str, range: std::ops::Range<usize>) -> Vec<std::ops::Range<usize>> {
    let mut lines = Vec::new();
    let mut start = text[..range.start].rfind(['\n', '\r']).map_or(0, |i| i + 1);
    while start <= range.end {
        let rest = &text[start..];
        let end = start + rest.find(['\n', '\r']).unwrap_or(rest.len());
        let from = range.start.max(start + block_prefix(&text[start..end]));
        let to = range.end.min(end);
        if from < to {
            let cut = &text[from..to];
            let lead = cut.len() - cut.trim_start().len();
            let content = from + lead..from + cut.trim_end().len();
            if !content.is_empty() {
                lines.push(content);
            }
        }
        let ending = if text[end..].starts_with("\r\n") {
            2
        } else {
            1
        };
        if end >= text.len() {
            break;
        }
        start = end + ending;
    }
    lines
}

/// How long the block markup at the start of `line` is: indentation and
/// quote markers, then a heading's hashes or a list marker (with a task
/// box) and the spaces after it.
fn block_prefix(line: &str) -> usize {
    let bytes = line.as_bytes();
    let mut at = 0;
    while matches!(bytes.get(at), Some(b' ' | b'\t' | b'>')) {
        at += 1;
    }
    let rest = &line[at..];
    let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
    let hashes = rest.bytes().take_while(|&b| b == b'#').count();
    let marker = if (1..=6).contains(&hashes) {
        hashes
    } else if rest.starts_with(['-', '+', '*']) {
        1
    } else if (1..=9).contains(&digits) && rest[digits..].starts_with(['.', ')']) {
        digits + 1
    } else {
        return at;
    };
    let after = &rest[marker..];
    if !(after.is_empty() || after.starts_with([' ', '\t'])) {
        return at;
    }
    let mut at = at + marker + after.len() - after.trim_start_matches([' ', '\t']).len();
    let task = &line[at..];
    if hashes == 0
        && task.len() >= 3
        && task.starts_with('[')
        && matches!(task.as_bytes()[1], b' ' | b'x' | b'X')
        && task.as_bytes()[2] == b']'
    {
        at += 3;
        at += line[at..].len() - line[at..].trim_start_matches([' ', '\t']).len();
    }
    at
}

/// The word the caret at `at` is strictly inside: word characters on both
/// sides (at a word's edge an empty pair is typed instead).
fn strictly_in_word(doc: &Doc, at: usize) -> Option<std::ops::Range<usize>> {
    let text = doc.text();
    let word = |c: char| c.is_alphanumeric() || c == '_';
    let before = text[..at].chars().next_back().is_some_and(word);
    let after = text[at..].chars().next().is_some_and(word);
    (before && after).then(|| word_at(doc, at, false))
}

/// Ctrl/Cmd+K (REFERENCE-001 section 12): a selected URL becomes
/// `[](url)` with the caret in the brackets; other selected text becomes
/// `[text]()` with the caret in the parentheses; nothing selected gives
/// `[]()` with the caret in the brackets.
pub fn link(doc: &mut Doc, now: Duration) {
    let range = doc.selection().range();
    let selected = &doc.text()[range.clone()];
    let (insert, caret) = if selected.is_empty() {
        ("[]()".to_owned(), 1)
    } else if is_url(selected) {
        (format!("[]({selected})"), 1)
    } else {
        let insert = format!("[{selected}]()");
        let caret = insert.len() - 1;
        (insert, caret)
    };
    let caret = range.start + caret;
    doc.apply(
        vec![Change {
            range,
            text: insert,
        }],
        Selection::caret(caret),
        Kind::Other,
        now,
    );
}

/// A click on a task's checkbox (REFERENCE-001 section 8): the box at
/// `task` (`[ ]`, `[x]` or `[X]`) checked or cleared, the selection kept.
pub fn toggle_task(doc: &mut Doc, task: std::ops::Range<usize>, now: Duration) {
    let checked = doc.text()[task.clone()] != *"[ ]";
    let mark = if checked { " " } else { "x" };
    let selection = doc.selection();
    doc.apply(
        vec![Change {
            range: task.start + 1..task.end - 1,
            text: mark.to_owned(),
        }],
        selection,
        Kind::Other,
        now,
    );
}

/// An absolute URL: a scheme, `:`, and no whitespace (what SilverBullet's
/// `new URL` check accepts, near enough).
fn is_url(text: &str) -> bool {
    let Some((scheme, rest)) = text.split_once(':') else {
        return false;
    };
    let mut chars = scheme.chars();
    chars.next().is_some_and(|c| c.is_ascii_alphabetic())
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '.' | '-'))
        && !rest.is_empty()
        && !text.contains(char::is_whitespace)
}

#[cfg(test)]
mod tests;
