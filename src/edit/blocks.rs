//! Line prefixes the toolbar toggles (PLAN-002): a heading's `#` run, a
//! bullet, a task box, a quote's `>`, on every line the selection touches,
//! as one undo step. A list or heading prefix a line already has is
//! replaced, so a bullet becomes a task in one click; the line's
//! indentation and quote markers stay.
use std::ops::Range;
use std::time::Duration;

use crate::doc::{Change, Doc, Kind, Selection};

/// A block prefix the toolbar toggles.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Block {
    /// Cycles no heading, `#`, `##`, `###`, no heading.
    Heading,
    /// `- ` in front of each line, or off.
    Bullet,
    /// `- [ ] ` in front of each line, or off.
    Task,
    /// `> ` in front of each line, or one level off.
    Quote,
}

/// What a line starts with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Marker {
    None,
    Bullet,
    Ordered,
    Task,
    Heading(u8),
}

/// A line's prefix: its quote markers, then its indentation, then its
/// marker with the spaces after it; each as a byte length.
struct Prefix {
    quote: usize,
    indent: usize,
    marker: Marker,
    len: usize,
}

fn prefix(line: &str) -> Prefix {
    let bytes = line.as_bytes();
    let mut quote = 0;
    loop {
        let spaces = bytes[quote..]
            .iter()
            .take(3)
            .take_while(|&&b| b == b' ')
            .count();
        if bytes.get(quote + spaces) != Some(&b'>') {
            break;
        }
        quote += spaces + 1;
        if bytes.get(quote) == Some(&b' ') {
            quote += 1;
        }
    }
    let rest = &line[quote..];
    let indent = rest.len() - rest.trim_start_matches([' ', '\t']).len();
    let rest = &rest[indent..];
    let spaced = |at: usize| {
        rest.get(at..)
            .is_some_and(|r| r.starts_with(' ') || r.is_empty())
    };
    let hashes = rest.bytes().take_while(|&b| b == b'#').count();
    let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
    let (marker, len) = if (1..=6).contains(&hashes) && spaced(hashes) {
        (Marker::Heading(hashes as u8), hashes + 1)
    } else if rest.starts_with(['-', '*', '+']) && spaced(1) {
        let task = rest
            .get(2..5)
            .is_some_and(|b| matches!(b, "[ ]" | "[x]" | "[X]"))
            && spaced(5);
        if task {
            (Marker::Task, 6)
        } else {
            (Marker::Bullet, 2)
        }
    } else if (1..=9).contains(&digits)
        && rest[digits..].starts_with(['.', ')'])
        && spaced(digits + 1)
    {
        (Marker::Ordered, digits + 2)
    } else {
        (Marker::None, 0)
    };
    Prefix {
        quote,
        indent,
        marker,
        len: len.min(rest.len()),
    }
}

/// Toggles `block` on the lines the selection touches (a selection ending
/// at a line's start leaves that line out).
pub fn toggle(doc: &mut Doc, block: Block, now: Duration) {
    let range = doc.selection().range();
    let first = doc.line_at(range.start);
    let mut last = doc.line_at(range.end);
    if last > first && range.end == doc.line_range(last).start {
        last -= 1;
    }
    let text = doc.text();
    let lines: Vec<(Range<usize>, Prefix)> = (first..=last)
        .map(|i| {
            let r = doc.line_range(i);
            let p = prefix(&text[r.clone()]);
            (r, p)
        })
        .collect();
    let blank = |r: &Range<usize>, p: &Prefix| text[r.start + p.quote..r.end].trim().is_empty();
    let filled: Vec<_> = lines.iter().filter(|(r, p)| !blank(r, p)).collect();
    let all = |marker: Marker| !filled.is_empty() && filled.iter().all(|(_, p)| p.marker == marker);
    let mut changes = Vec::new();
    // The marker of a line, from after its indentation, replaced.
    let mut set = |r: &Range<usize>, p: &Prefix, new: &str| {
        let at = r.start + p.quote + p.indent;
        if text[at..at + p.len] != *new {
            changes.push(Change {
                range: at..at + p.len,
                text: new.to_owned(),
            });
        }
    };
    match block {
        Block::Bullet | Block::Task => {
            let (marker, new) = match block {
                Block::Bullet => (Marker::Bullet, "- "),
                _ => (Marker::Task, "- [ ] "),
            };
            let new = if all(marker) { "" } else { new };
            for (r, p) in &filled {
                set(r, p, new);
            }
        }
        Block::Heading => {
            // The next level after the first line's, for every line.
            let level = match filled.first().map(|(_, p)| p.marker) {
                Some(Marker::Heading(n)) if n < 3 => n + 1,
                Some(Marker::Heading(_)) => 0,
                _ => 1,
            };
            let new = if level == 0 {
                String::new()
            } else {
                format!("{} ", "#".repeat(level.into()))
            };
            for (r, p) in &filled {
                set(r, p, &new);
            }
        }
        Block::Quote => {
            let quoted = !filled.is_empty() && filled.iter().all(|(_, p)| p.quote > 0);
            for (r, p) in &lines {
                if quoted {
                    // One level off: the first `>` and the space after it.
                    let line = &text[r.clone()];
                    if let Some(at) = line.find('>').filter(|&at| at < p.quote) {
                        let end = at + 1 + usize::from(line[at + 1..].starts_with(' '));
                        changes.push(Change::delete(r.start + at..r.start + end));
                    }
                } else if blank(r, p) && p.quote == 0 {
                    changes.push(Change::insert(r.start, ">".to_owned()));
                } else {
                    changes.push(Change::insert(r.start, "> ".to_owned()));
                }
            }
        }
    }
    if changes.is_empty() {
        return;
    }
    changes.sort_by_key(|c| c.range.start);
    let selection = doc.selection();
    let selection = Selection {
        anchor: moved(&changes, selection.anchor),
        head: moved(&changes, selection.head),
    };
    doc.apply(changes, selection, Kind::Other, now);
}

/// Where offset `at` lands after `changes` (sorted): inside a replaced
/// prefix, after the new one.
fn moved(changes: &[Change], at: usize) -> usize {
    let mut shift = 0isize;
    for change in changes {
        if at < change.range.start {
            break;
        }
        if at < change.range.end {
            return change.range.start.wrapping_add_signed(shift) + change.text.len();
        }
        shift += change.text.len() as isize - change.range.len() as isize;
    }
    at.wrapping_add_signed(shift)
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{Block, toggle};
    use crate::doc::{Doc, Selection};

    /// `block` toggled over `anchor..head` of `text`: the text and caret.
    fn apply(text: &str, (anchor, head): (usize, usize), block: Block) -> (String, usize) {
        let mut doc = Doc::new(text.into());
        doc.set_selection(Selection { anchor, head });
        toggle(&mut doc, block, Duration::ZERO);
        (doc.text().to_owned(), doc.selection().head)
    }

    #[test]
    fn bullets_tasks_and_headings_toggle_and_replace_each_other() {
        assert_eq!(
            apply("milk\neggs\n", (0, 9), Block::Bullet).0,
            "- milk\n- eggs\n"
        );
        assert_eq!(
            apply("- milk\n- eggs\n", (0, 13), Block::Bullet).0,
            "milk\neggs\n"
        );
        assert_eq!(
            apply("- milk\n", (3, 3), Block::Task),
            ("- [ ] milk\n".into(), 7)
        );
        assert_eq!(apply("- [x] milk\n", (0, 0), Block::Task).0, "milk\n");
        assert_eq!(apply("1. one\n", (0, 0), Block::Bullet).0, "- one\n");
        // Indentation and quote markers stay; blank lines are skipped.
        assert_eq!(
            apply("> a\n\n  b\n", (0, 8), Block::Bullet).0,
            "> - a\n\n  - b\n"
        );
        // Headings cycle, and replace a list marker.
        let mut text = String::from("Title\n");
        for expected in ["# Title\n", "## Title\n", "### Title\n", "Title\n"] {
            text = apply(&text, (0, 0), Block::Heading).0;
            assert_eq!(text, expected);
        }
        assert_eq!(apply("- Title\n", (0, 0), Block::Heading).0, "# Title\n");
    }

    #[test]
    fn quotes_add_and_take_one_level_on_every_line() {
        assert_eq!(apply("a\n\nb\n", (0, 4), Block::Quote).0, "> a\n>\n> b\n");
        assert_eq!(apply("> a\n>\n> b\n", (0, 9), Block::Quote).0, "a\n\nb\n");
        assert_eq!(apply("> > a\n", (4, 4), Block::Quote), ("> a\n".into(), 2));
        // A selection ending at a line's start leaves that line out.
        assert_eq!(apply("a\nb\n", (0, 2), Block::Quote).0, "> a\nb\n");
    }
}
