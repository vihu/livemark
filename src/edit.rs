//! Editing commands over a [`Doc`]: caret motions that need no layout, and
//! typing, deleting, Enter and paste as transactions (REFERENCE-001
//! sections 13, 15 and 18). Motions that need layout (up, down, pages) are
//! the widget's.
use std::ops::Range;
use std::time::Duration;

use unicode_segmentation::UnicodeSegmentation;

use crate::doc::{Change, Doc, Kind, Selection};

/// A caret motion that needs no layout.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Motion {
    /// One grapheme back; `\r\n` is one.
    Left,
    /// One grapheme on.
    Right,
    /// Back over spaces, then a run of word characters or of punctuation.
    WordLeft,
    /// On over spaces, then a run of word characters or of punctuation.
    WordRight,
    /// The start of the source line.
    LineStart,
    /// The end of the source line, before its ending.
    LineEnd,
    /// The start of the document.
    DocumentStart,
    /// The end of the document.
    DocumentEnd,
}

/// Where `motion` takes a caret at `from`.
pub fn target(doc: &Doc, from: usize, motion: Motion) -> usize {
    let text = doc.text();
    match motion {
        Motion::Left => text[..from]
            .grapheme_indices(true)
            .next_back()
            .map_or(0, |(i, _)| i),
        Motion::Right => text[from..]
            .graphemes(true)
            .next()
            .map_or(from, |g| from + g.len()),
        Motion::WordLeft => word_left(text, from),
        Motion::WordRight => word_right(text, from),
        Motion::LineStart => doc.line_range(doc.line_at(from)).start,
        Motion::LineEnd => doc.line_range(doc.line_at(from)).end,
        Motion::DocumentStart => 0,
        Motion::DocumentEnd => text.len(),
    }
}

/// Moves the caret, or with `extend` only the selection's head. Left and
/// Right without `extend` collapse a selection to its edge instead
/// (CodeMirror's `cursorCharLeft`).
pub fn go(doc: &mut Doc, motion: Motion, extend: bool) {
    let selection = doc.selection();
    let range = selection.range();
    let head = match motion {
        Motion::Left if !extend && !range.is_empty() => range.start,
        Motion::Right if !extend && !range.is_empty() => range.end,
        _ => target(doc, selection.head, motion),
    };
    doc.set_selection(if extend {
        Selection {
            anchor: selection.anchor,
            head,
        }
    } else {
        Selection::caret(head)
    });
}

/// Home and End (REFERENCE-001 section 13, CodeMirror's
/// `moveByLineBoundary`): to the edge of the visual row `row` the caret is
/// on; from an edge already reached, to the edge of the source line. Home
/// stops at the end of the indentation unless the caret is already there.
pub fn line_boundary(doc: &Doc, head: usize, row: Range<usize>, forward: bool) -> usize {
    let line = doc.line_range(doc.line_at(head));
    let (edge, line_edge) = if forward {
        (row.end, line.end)
    } else {
        (row.start, line.start)
    };
    let mut to = if edge == head { line_edge } else { edge };
    if !forward && to == line.start && !line.is_empty() {
        let text = &doc.text()[line.clone()];
        let space = text.len() - text.trim_start_matches([' ', '\t']).len();
        if space > 0 && head != line.start + space {
            to = line.start + space;
        }
    }
    to
}

/// The word, run of spaces or run of punctuation a double click at `at`
/// selects (REFERENCE-001 section 14): the grapheme before `at` when
/// `before` (the click was on the right half of it), else the one after,
/// grown in both directions within the line while the class holds.
pub fn word_at(doc: &Doc, at: usize, before: bool) -> Range<usize> {
    let line = doc.line_range(doc.line_at(at));
    let text = &doc.text()[line.clone()];
    let at = at.min(line.end) - line.start;
    if text.is_empty() {
        return line.start..line.start;
    }
    let (mut from, mut to) = if (before && at > 0) || at == text.len() {
        let prev = text[..at]
            .grapheme_indices(true)
            .next_back()
            .map_or(0, |(i, _)| i);
        (prev, at)
    } else {
        let next = text[at..]
            .graphemes(true)
            .next()
            .map_or(at, |g| at + g.len());
        (at, next)
    };
    let kind = class(text[from..].chars().next().unwrap_or(' '));
    while let Some((i, g)) = text[..from].grapheme_indices(true).next_back() {
        if g.chars().next().map(class) != Some(kind) {
            break;
        }
        from = i;
    }
    while let Some(g) = text[to..].graphemes(true).next() {
        if g.chars().next().map(class) != Some(kind) {
            break;
        }
        to += g.len();
    }
    line.start + from..line.start + to
}

/// The source line at `at` with its line ending: what a triple click
/// selects (REFERENCE-001 section 14).
pub fn line_with_ending(doc: &Doc, at: usize) -> Range<usize> {
    let index = doc.line_at(at);
    let start = doc.line_range(index).start;
    let end = if index + 1 < doc.line_count() {
        doc.line_range(index + 1).start
    } else {
        doc.text().len()
    };
    start..end
}

/// What copy takes and cut removes: the selection, or with nothing
/// selected the caret's line (its text without the ending; cut removes the
/// ending too), and whether it was such a line-wise copy (REFERENCE-001
/// section 18, CodeMirror's `copiedRange`).
pub fn copied(doc: &Doc) -> (String, Range<usize>, bool) {
    let range = doc.selection().range();
    if range.is_empty() {
        let line = doc.line_range(doc.line_at(range.start));
        let text = doc.text()[line].to_owned();
        (text, line_with_ending(doc, range.start), true)
    } else {
        (doc.text()[range.clone()].to_owned(), range, false)
    }
}

/// Cut after copying: removes what [`copied`] took, the line and its
/// ending when nothing was selected.
pub fn cut(doc: &mut Doc, now: Duration) {
    let (_, range, _) = copied(doc);
    if !range.is_empty() {
        let start = range.start;
        doc.apply(
            vec![Change::delete(range)],
            Selection::caret(start),
            Kind::Delete,
            now,
        );
    }
}

/// Pastes a line-wise copy with nothing selected: `text` as a new line
/// above the caret's line, the caret moving with its text (CodeMirror's
/// `doPaste`).
pub fn paste_line(doc: &mut Doc, text: &str, now: Duration) {
    let ending = line_ending(doc.text());
    let text = convert_endings(text, ending) + ending;
    let head = doc.selection().head;
    let start = doc.line_range(doc.line_at(head)).start;
    let caret = head + text.len();
    doc.apply(
        vec![Change::insert(start, text)],
        Selection::caret(caret),
        Kind::Other,
        now,
    );
}

/// Replaces the selection with typed `text`.
pub fn type_text(doc: &mut Doc, text: &str, now: Duration) {
    replace_selection(doc, text, Kind::Type, now);
}

/// Enter outside lists, quotes and code: the document's line ending
/// (REFERENCE-001 section 13).
pub fn enter(doc: &mut Doc, now: Duration) {
    let ending = line_ending(doc.text());
    replace_selection(doc, ending, Kind::Other, now);
}

/// Replaces the selection with pasted `text`, its line breaks turned into
/// the document's line ending (REFERENCE-001 section 18).
pub fn paste(doc: &mut Doc, text: &str, now: Duration) {
    let text = convert_endings(text, line_ending(doc.text()));
    replace_selection(doc, &text, Kind::Other, now);
}

/// `text` with every line break (`\r\n`, `\r`, `\n`) as `ending`.
fn convert_endings(text: &str, ending: &str) -> String {
    let text = text.replace("\r\n", "\n").replace('\r', "\n");
    if ending == "\n" {
        text
    } else {
        text.replace('\n', ending)
    }
}

/// Deletes the selection, or from the caret to where `motion` takes it
/// (Backspace is [`Motion::Left`], Delete [`Motion::Right`]).
pub fn delete(doc: &mut Doc, motion: Motion, now: Duration) {
    let range = doc.selection().range();
    let range = if range.is_empty() {
        let to = target(doc, range.start, motion);
        to.min(range.start)..to.max(range.start)
    } else {
        range
    };
    if !range.is_empty() {
        let start = range.start;
        doc.apply(
            vec![Change::delete(range)],
            Selection::caret(start),
            Kind::Delete,
            now,
        );
    }
}

fn replace_selection(doc: &mut Doc, text: &str, kind: Kind, now: Duration) {
    let range = doc.selection().range();
    let caret = range.start + text.len();
    doc.apply(
        vec![Change {
            range,
            text: text.to_owned(),
        }],
        Selection::caret(caret),
        kind,
        now,
    );
}

/// The document's line ending: its first one, else `\n`, so edits never
/// mix endings.
pub fn line_ending(text: &str) -> &'static str {
    match text.find(['\n', '\r']).map(|i| &text.as_bytes()[i..]) {
        Some([b'\r', b'\n', ..]) => "\r\n",
        Some([b'\r', ..]) => "\r",
        _ => "\n",
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Class {
    Space,
    Break,
    Word,
    Other,
}

fn class(c: char) -> Class {
    match c {
        '\n' | '\r' => Class::Break,
        c if c.is_whitespace() => Class::Space,
        c if c.is_alphanumeric() || c == '_' => Class::Word,
        _ => Class::Other,
    }
}

/// Skips spaces, then one run of a class; a line ending is a step alone
/// (REFERENCE-001 section 13).
fn word_right(text: &str, from: usize) -> usize {
    let mut chars = text[from..].char_indices().peekable();
    let mut end = from;
    if chars.peek().is_some_and(|&(_, c)| class(c) == Class::Break) {
        return text[from..]
            .graphemes(true)
            .next()
            .map_or(from, |g| from + g.len());
    }
    while let Some(&(i, c)) = chars.peek() {
        if class(c) != Class::Space {
            break;
        }
        end = from + i + c.len_utf8();
        chars.next();
    }
    let Some(&(_, first)) = chars.peek() else {
        return end;
    };
    let run = class(first);
    if run == Class::Break {
        return end;
    }
    for (i, c) in chars {
        if class(c) != run {
            break;
        }
        end = from + i + c.len_utf8();
    }
    end
}

fn word_left(text: &str, from: usize) -> usize {
    let mut chars = text[..from].char_indices().rev().peekable();
    let mut start = from;
    if chars.peek().is_some_and(|&(_, c)| class(c) == Class::Break) {
        return text[..from]
            .grapheme_indices(true)
            .next_back()
            .map_or(0, |(i, _)| i);
    }
    while let Some(&(i, c)) = chars.peek() {
        if class(c) != Class::Space {
            break;
        }
        start = i;
        chars.next();
    }
    let Some(&(_, first)) = chars.peek() else {
        return start;
    };
    let run = class(first);
    if run == Class::Break {
        return start;
    }
    for (i, c) in chars {
        if class(c) != run {
            break;
        }
        start = i;
    }
    start
}

#[cfg(test)]
mod tests;
