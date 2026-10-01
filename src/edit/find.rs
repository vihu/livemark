//! Find and replace (REFERENCE-001 section 16), after CodeMirror's search
//! commands (`search` `dist/index.js:850-977`, MIT): matches in the source
//! text, markers included; next and previous wrap around; replace changes
//! the selected match only; replace all is one undo step.
use std::ops::Range;
use std::time::Duration;

use crate::doc::{Change, Doc, Kind, Selection};

/// What to look for. Not a regular expression, not whole words
/// (CodeMirror's defaults).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Query {
    /// The text to find.
    pub text: String,
    /// Match case exactly; off by default.
    pub case_sensitive: bool,
}

impl Query {
    /// Every match in `text`, in order and not overlapping.
    pub fn matches(&self, text: &str) -> Vec<Range<usize>> {
        let query = self.text.as_str();
        if query.is_empty() {
            return Vec::new();
        }
        if self.case_sensitive {
            return text
                .match_indices(query)
                .map(|(at, found)| at..at + found.len())
                .collect();
        }
        let mut matches = Vec::new();
        let mut at = 0;
        while at < text.len() {
            match self.match_at(text, at) {
                Some(end) => {
                    matches.push(at..end);
                    at = end;
                }
                None => at += text[at..].chars().next().map_or(1, char::len_utf8),
            }
        }
        matches
    }

    /// Where a case-insensitive match starting at `at` ends, comparing
    /// character by character.
    fn match_at(&self, text: &str, at: usize) -> Option<usize> {
        if self.text.is_ascii() {
            let end = at + self.text.len();
            let found = text.as_bytes().get(at..end)?;
            return found
                .eq_ignore_ascii_case(self.text.as_bytes())
                .then_some(end);
        }
        let mut chars = text[at..].char_indices();
        for wanted in self.text.chars() {
            let (_, found) = chars.next()?;
            if !found.to_lowercase().eq(wanted.to_lowercase()) {
                return None;
            }
        }
        Some(chars.next().map_or(text.len(), |(i, _)| at + i))
    }
}

/// The first match from `to` on, else the first in the document, unless it
/// is the selection `from..to` itself (CodeMirror's `nextMatch`).
pub fn next_match(matches: &[Range<usize>], from: usize, to: usize) -> Option<Range<usize>> {
    let i = matches.partition_point(|m| m.start < to);
    let found = matches.get(i).or(matches.first())?.clone();
    (found != (from..to)).then_some(found)
}

/// The last match ending by `from`, else the last in the document, unless
/// it is the selection `from..to` itself (CodeMirror's `prevMatch`).
pub fn prev_match(matches: &[Range<usize>], from: usize, to: usize) -> Option<Range<usize>> {
    let i = matches.partition_point(|m| m.end <= from);
    let found = i
        .checked_sub(1)
        .and_then(|i| matches.get(i))
        .or(matches.last())?
        .clone();
    (found != (from..to)).then_some(found)
}

/// Selects the next match after the selection (`forward`) or the one
/// before it, wrapping around. Returns whether there was one.
pub fn find(doc: &mut Doc, query: &Query, forward: bool) -> bool {
    let matches = query.matches(doc.text());
    let range = doc.selection().range();
    let found = if forward {
        next_match(&matches, range.start, range.end)
    } else {
        prev_match(&matches, range.start, range.end)
    };
    match found {
        Some(found) => {
            doc.set_selection(Selection {
                anchor: found.start,
                head: found.end,
            });
            true
        }
        None => false,
    }
}

/// Replaces the selected match with `replacement` and selects the next
/// one; with no match selected, only selects the next one (CodeMirror's
/// `replaceNext`).
pub fn replace(doc: &mut Doc, query: &Query, replacement: &str, now: Duration) {
    let matches = query.matches(doc.text());
    let range = doc.selection().range();
    let Some(found) = next_match(&matches, range.start, range.start) else {
        return;
    };
    if found != range {
        doc.set_selection(Selection {
            anchor: found.start,
            head: found.end,
        });
        return;
    }
    let next = next_match(&matches, found.start, found.end);
    let delta = replacement.len() as isize - found.len() as isize;
    let selection = match next {
        Some(next) if next.start >= found.end => Selection {
            anchor: next.start.wrapping_add_signed(delta),
            head: next.end.wrapping_add_signed(delta),
        },
        Some(next) => Selection {
            anchor: next.start,
            head: next.end,
        },
        None => Selection::caret(found.start + replacement.len()),
    };
    doc.apply(
        vec![Change {
            range: found,
            text: replacement.to_owned(),
        }],
        selection,
        Kind::Other,
        now,
    );
}

/// Replaces every match with `replacement` as one undo step and returns
/// how many there were.
pub fn replace_all(doc: &mut Doc, query: &Query, replacement: &str, now: Duration) -> usize {
    let matches = query.matches(doc.text());
    if matches.is_empty() {
        return 0;
    }
    let mut delta = 0isize;
    let head = doc.selection().head;
    let mut caret = head;
    for found in &matches {
        if found.end <= head {
            delta += replacement.len() as isize - found.len() as isize;
            caret = head.wrapping_add_signed(delta);
        } else if found.start < head {
            caret = found.start.wrapping_add_signed(delta) + replacement.len();
        }
    }
    let changes = matches
        .iter()
        .map(|found| Change {
            range: found.clone(),
            text: replacement.to_owned(),
        })
        .collect();
    doc.apply(changes, Selection::caret(caret), Kind::Other, now);
    matches.len()
}

#[cfg(test)]
mod tests;
