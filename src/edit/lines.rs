//! Line commands from CodeMirror's default keymap (`commands`
//! `dist/index.js:1385-1471`, `1537-1567`, keys at `1799-1812`, MIT), which
//! Keeprs' web editor has: move or copy the selected lines up or down,
//! delete them, open a blank line below. Each is one undo step. Line
//! endings stay where they are: moved lines trade places with their
//! neighbour around the ending between them.
use std::ops::Range;
use std::time::Duration;

use super::line_ending;
use crate::doc::{Change, Doc, Kind, Selection};

/// The first and last line the selection covers; a selection ending at a
/// line's start leaves that line out (CodeMirror's `selectedLineBlocks`).
fn block(doc: &Doc) -> (usize, usize) {
    let range = doc.selection().range();
    let first = doc.line_at(range.start);
    let mut last = doc.line_at(range.end);
    if !range.is_empty() && range.end == doc.line_range(last).start {
        last -= 1;
    }
    (first, last)
}

/// The text of lines `first` to `last`, without the last one's ending.
fn span(doc: &Doc, first: usize, last: usize) -> Range<usize> {
    doc.line_range(first).start..doc.line_range(last).end
}

/// The line ending after line `index`, else the document's.
fn ending_after(doc: &Doc, index: usize) -> &str {
    let text = doc.text();
    if index + 1 < doc.line_count() {
        &text[doc.line_range(index).end..doc.line_range(index + 1).start]
    } else {
        line_ending(text)
    }
}

/// Alt+Up and Alt+Down (`moveLine`): the selected lines trade places with
/// the line above or below, and the selection goes with them. A selection
/// end at the start of the line after them stays after them, past
/// whichever line ending now follows.
pub fn move_lines(doc: &mut Doc, down: bool, now: Duration) {
    let (first, last) = block(doc);
    let lines = span(doc, first, last);
    let text = doc.text();
    // The text replaced, what replaces it, where the lines start in it and
    // the length of the line ending after them there.
    let (range, swapped, start, after) = if down {
        if last + 1 >= doc.line_count() {
            return;
        }
        let next = doc.line_range(last + 1);
        let ending = &text[lines.end..next.start];
        let swapped = format!("{}{ending}{}", &text[next.clone()], &text[lines.clone()]);
        let after = if last + 2 < doc.line_count() {
            doc.line_range(last + 2).start - next.end
        } else {
            0
        };
        let start = lines.start + next.end - lines.end;
        (lines.start..next.end, swapped, start, after)
    } else {
        if first == 0 {
            return;
        }
        let prev = doc.line_range(first - 1);
        let ending = &text[prev.end..lines.start];
        let swapped = format!("{}{ending}{}", &text[lines.clone()], &text[prev.clone()]);
        (prev.start..lines.end, swapped, prev.start, ending.len())
    };
    let moved = |at: usize| {
        if at <= lines.end {
            start + at - lines.start
        } else {
            start + lines.len() + after
        }
    };
    let selection = doc.selection();
    doc.apply(
        vec![Change {
            range,
            text: swapped,
        }],
        Selection {
            anchor: moved(selection.anchor),
            head: moved(selection.head),
        },
        Kind::Other,
        now,
    );
}

/// Shift+Alt+Up and Shift+Alt+Down (`copyLine`): a copy of the selected
/// lines above or below them; the selection stays in the upper copy going
/// up and in the lower one going down.
pub fn copy_lines(doc: &mut Doc, down: bool, now: Duration) {
    let (first, last) = block(doc);
    let lines = span(doc, first, last);
    let ending = ending_after(doc, last);
    let copy = &doc.text()[lines.clone()];
    let (at, insert) = if down {
        (lines.start, format!("{copy}{ending}"))
    } else {
        (lines.end, format!("{ending}{copy}"))
    };
    let moved = |pos: usize| {
        if pos > at || (down && pos == at) {
            pos + insert.len()
        } else {
            pos
        }
    };
    let selection = doc.selection();
    let selection = Selection {
        anchor: moved(selection.anchor),
        head: moved(selection.head),
    };
    doc.apply(
        vec![Change::insert(at, insert)],
        selection,
        Kind::Other,
        now,
    );
}

/// Ctrl/Cmd+Shift+K (`deleteLine`): the selected lines go with the line
/// ending before them (after them for the first line). The caret keeps its
/// column on the line below, as CodeMirror moves it down a row first.
pub fn delete_lines(doc: &mut Doc, now: Duration) {
    let (first, last) = block(doc);
    let mut range = span(doc, first, last);
    let below = (last + 1 < doc.line_count()).then(|| doc.line_range(last + 1));
    if first > 0 {
        range.start = doc.line_range(first - 1).end;
    } else if let Some(below) = &below {
        range.end = below.start;
    }
    let text = doc.text();
    let head = doc.selection().head;
    let column = text[doc.line_range(doc.line_at(head)).start..head]
        .chars()
        .count();
    let caret = match below {
        Some(below) => {
            let at = text[below.clone()]
                .char_indices()
                .nth(column)
                .map_or(below.end, |(i, _)| below.start + i);
            at - range.len()
        }
        None => range.start,
    };
    doc.apply(
        vec![Change::delete(range)],
        Selection::caret(caret),
        Kind::Other,
        now,
    );
}

/// Ctrl/Cmd+Enter (`insertBlankLine`): a new line after the one the
/// selection ends on, indented like it, whatever the markup around (the
/// markdown language gives no indentation, so CodeMirror copies the
/// line's). A line of only whitespace is emptied.
pub fn blank_line(doc: &mut Doc, now: Duration) {
    let line = doc.line_range(doc.line_at(doc.selection().range().end));
    let text = doc.text();
    let content = &text[line.clone()];
    let indent = &content[..content.len() - content.trim_start_matches([' ', '\t']).len()];
    let start = if indent.len() == content.len() {
        line.start
    } else {
        line.end
    };
    let insert = format!("{}{indent}", line_ending(text));
    let caret = start + insert.len();
    doc.apply(
        vec![Change {
            range: start..line.end,
            text: insert,
        }],
        Selection::caret(caret),
        Kind::Other,
        now,
    );
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{blank_line, copy_lines, delete_lines, move_lines};
    use crate::doc::{Doc, Selection};

    /// Runs `command` on `text` with the selection `anchor..head` and
    /// returns the text with `|` at the caret, or `[` and `]` around the
    /// selection.
    fn run(text: &str, anchor: usize, head: usize, command: impl FnOnce(&mut Doc)) -> String {
        let mut doc = Doc::new(text.into());
        doc.set_selection(Selection { anchor, head });
        command(&mut doc);
        let range = doc.selection().range();
        let mut out = doc.text().to_owned();
        if range.is_empty() {
            out.insert(range.start, '|');
        } else {
            out.insert(range.end, ']');
            out.insert(range.start, '[');
        }
        out
    }

    const NOW: Duration = Duration::ZERO;

    #[test]
    fn moving_lines_trades_places_and_keeps_the_endings() {
        let down = |d: &mut Doc| move_lines(d, true, NOW);
        let up = |d: &mut Doc| move_lines(d, false, NOW);
        assert_eq!(run("a\nbc\nd", 3, 3, down), "a\nd\nb|c");
        assert_eq!(run("a\nbc\nd", 3, 3, up), "b|c\na\nd");
        assert_eq!(run("a\nb", 0, 0, up), "|a\nb", "first line up");
        assert_eq!(run("a\nb", 2, 2, down), "a\n|b", "last line down");
        // Two lines selected to the start of the third: the third stays.
        assert_eq!(run("a\nb\nc\nd", 0, 4, down), "c\n[a\nb\n]d");
        assert_eq!(run("a\r\nb\nc", 3, 3, up), "|b\r\na\nc", "endings stay put");
        assert_eq!(
            run("é\nb\r\nc", 3, 6, up),
            "[b\n]é\r\nc",
            "a line selected with its ending, moved above a shorter one"
        );
    }

    #[test]
    fn copying_lines_keeps_the_selection_in_the_right_copy() {
        let down = |d: &mut Doc| copy_lines(d, true, NOW);
        let up = |d: &mut Doc| copy_lines(d, false, NOW);
        assert_eq!(run("ab\nc", 1, 1, down), "ab\na|b\nc");
        assert_eq!(run("ab\nc", 1, 1, up), "a|b\nab\nc");
        assert_eq!(
            run("a\r\nb", 3, 3, down),
            "a\r\nb\r\n|b",
            "the document's ending"
        );
    }

    #[test]
    fn deleting_lines_keeps_the_column_on_the_line_below() {
        let delete = |d: &mut Doc| delete_lines(d, NOW);
        assert_eq!(run("ab\ncd\nef", 4, 4, delete), "ab\ne|f");
        assert_eq!(run("ab\ncd", 1, 1, delete), "c|d", "first line");
        assert_eq!(run("ab\ncd", 4, 4, delete), "ab|", "last line");
        assert_eq!(run("ab", 1, 1, delete), "|");
        assert_eq!(run("é\nab", 2, 2, delete), "a|b", "columns in characters");
    }

    #[test]
    fn a_blank_line_opens_below_with_the_same_indent() {
        let blank = |d: &mut Doc| blank_line(d, NOW);
        assert_eq!(run("  - item\nnext", 4, 4, blank), "  - item\n  |\nnext");
        assert_eq!(run("a\r\nb", 0, 0, blank), "a\r\n|\r\nb");
        assert_eq!(run("x\n   ", 4, 4, blank), "x\n\n   |", "whitespace only");
    }
}
