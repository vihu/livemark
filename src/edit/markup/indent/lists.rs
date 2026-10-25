//! What moving list items reads and works out: indentation as columns,
//! markers' widths, the place a line moves to, and an item's lines moved
//! with its first.
use std::borrow::Cow;
use std::ops::Range;

use super::super::context::{Blocks, Container, item_number};
use super::super::{next_line, quote_prefix};
use super::{columns, marker_width};
use crate::doc::Change;

/// What moving list items reads: the text and its blocks, how many quotes
/// the list is in (lines move after their `>`), whether its lists are
/// indented with tabs, and whether it is ordered.
pub(super) struct Lists<'a> {
    pub(super) text: &'a str,
    pub(super) blocks: &'a Blocks,
    pub(super) quotes: usize,
    pub(super) tabs: bool,
    pub(super) ordered: bool,
}

impl<'a> Lists<'a> {
    /// How far `start` is into its line, in bytes.
    pub(super) fn column(&self, start: usize) -> usize {
        start - self.text[..start].rfind(['\n', '\r']).map_or(0, |i| i + 1)
    }

    /// Markers in front of a container on its line: a parent item's.
    pub(super) fn markers(&self, start: usize) -> bool {
        self.text[start - self.column(start)..start]
            .bytes()
            .any(|b| !matches!(b, b' ' | b'\t' | b'>'))
    }

    /// Where a container's line's indentation starts, and the indentation,
    /// read with the markers in front of it on its line as spaces.
    pub(super) fn indent(&self, start: usize) -> (usize, Cow<'a, str>) {
        let line = start - self.column(start);
        let rest = &self.text[line..];
        let row = &rest[..rest.find(['\n', '\r']).unwrap_or(rest.len())];
        let row: Cow<'a, str> = if self.markers(start) {
            let blanked = row.bytes().enumerate().map(|(i, b)| match b {
                b' ' | b'\t' | b'>' => b,
                _ if line + i < start => b' ',
                _ => b,
            });
            String::from_utf8(blanked.collect()).map_or(Cow::Borrowed(row), Cow::Owned)
        } else {
            Cow::Borrowed(row)
        };
        let at = quote_prefix(&row, self.quotes);
        let blank = row[at..].len() - row[at..].trim_start_matches([' ', '\t']).len();
        let blank = match row {
            Cow::Borrowed(row) => Cow::Borrowed(&row[at..at + blank]),
            Cow::Owned(row) => Cow::Owned(row[at..at + blank].to_owned()),
        };
        (line + at, blank)
    }

    /// Indentation `blank` at `at` in columns from where its quotes' text
    /// starts: a `>` with no space after it takes the first column of what
    /// follows as its own (CommonMark 5.1); a tab goes to the next stop.
    pub(super) fn rel(&self, at: usize, blank: &str) -> usize {
        let start = columns(0, &self.text[at - self.column(at)..at]);
        let bare = blank.starts_with([' ', '\t']) && self.text[..at].ends_with('>');
        columns(start, blank) - start - usize::from(bare)
    }

    /// The columns of the marker at `from` (with the space or tab after
    /// it), numbered `number` when given, on a line whose text starts
    /// `blank` after `at`.
    pub(super) fn marker(&self, from: usize, at: usize, blank: &str, number: Option<u64>) -> usize {
        let item = &self.text[from..];
        let width = marker_width(item);
        let shown = &item[..width.min(item.find(['\n', '\r']).unwrap_or(item.len()))];
        let pad = width - shown.len();
        let digits = shown.bytes().take_while(u8::is_ascii_digit).count();
        let shown = match number {
            Some(n) if digits > 0 => format!("{n}{}", &shown[digits..]),
            _ => shown.to_owned(),
        };
        self.rel(at, &format!("{blank}{shown}")) - self.rel(at, blank) + pad
    }

    /// `rel` columns of spaces at `at`, one more after a bare `>`.
    pub(super) fn spaces(&self, at: usize, rel: usize) -> String {
        " ".repeat(rel + usize::from(rel > 0 && self.text[..at].ends_with('>')))
    }

    /// Under an item's text, from indentation `blank` at `at`: a tab where
    /// tabs are used and it reaches that far, else spaces as wide as the
    /// item's marker. The text and its columns.
    pub(super) fn under(&self, at: usize, blank: &str, marker: usize) -> (String, usize) {
        let want = self.rel(at, blank) + marker;
        let tab = format!("{blank}\t");
        if self.tabs && self.rel(at, &tab) >= want {
            let columns = self.rel(at, &tab);
            (tab, columns)
        } else if blank.is_empty() {
            (self.spaces(at, want), want)
        } else {
            (format!("{blank}{}", " ".repeat(marker)), want)
        }
    }

    /// The byte after the number of the ordered item at `start` (its `.`
    /// or `)`), when it is one.
    pub(super) fn delimiter(&self, start: usize) -> Option<Option<u8>> {
        item_number(self.text, start)
            .map(|(digits, _)| self.text.as_bytes().get(digits.end).copied())
    }

    /// Item `p`'s first nested item, and the number an ordered item from
    /// `start` takes after it: on from the ordered list that ends `p` when
    /// it joins that list (MarkText's `tabCtrl.js:182-196`), else 1.
    pub(super) fn nested(&self, p: usize, start: usize) -> (Option<&'a Container>, u64) {
        let blocks = self.blocks;
        let p = &blocks.containers[p];
        let inside = |c: &&Container| {
            c.list.is_some() && c.range.start > p.range.start && c.range.start <= p.range.end
        };
        let number = blocks
            .containers
            .iter()
            .filter(|c| inside(c) && c.range.end == p.range.end)
            .min_by_key(|c| c.range.start)
            .filter(|c| {
                self.ordered
                    && c.list.is_some_and(|l| blocks.lists[l].ordered)
                    && self.delimiter(c.range.start) == self.delimiter(start)
            })
            .and_then(|c| item_number(self.text, c.range.start))
            .map_or(1, |(_, n)| n + 1);
        (blocks.containers.iter().find(inside), number)
    }

    /// A line's indentation `blank` at `at` that was `own` plus more, put
    /// at `goal` columns: `own` replaced by `to` where that lands there
    /// (tabs kept), else spaces. The change and the new indentation.
    pub(super) fn place(
        &self,
        at: usize,
        blank: &str,
        own: &str,
        to: &str,
        goal: usize,
    ) -> (Range<usize>, String, String) {
        if let Some(rest) = blank.strip_prefix(own) {
            let kept = format!("{to}{rest}");
            if self.rel(at, &kept) == goal {
                return (at..at + own.len(), to.to_owned(), kept);
            }
        }
        let new = self.spaces(at, goal);
        (at..at + blank.len(), new.clone(), new)
    }

    /// How far the text of the item whose line's indentation `own` at `at`
    /// becomes `new` moves: with its marker, which can change width there (a
    /// tab after it, a number of `number`'s digits).
    pub(super) fn text_shift(&self, at: usize, own: &str, new: &str, number: Option<u64>) -> isize {
        let start = at + own.len();
        (self.rel(at, new) + self.marker(start, at, new, number)) as isize
            - (self.rel(at, own) + self.marker(start, at, own, None)) as isize
    }

    /// The lines of the item from `from` to `end` move as its first line
    /// goes to `to`, `to_rel` columns in, numbered `number` when given;
    /// lines after the first move with its text.
    pub(super) fn retarget(
        &self,
        from: usize,
        end: usize,
        (to, to_rel): (&str, usize),
        number: Option<u64>,
        changes: &mut Vec<Change>,
    ) {
        let text = self.text;
        let first = from - self.column(from);
        let (at, own) = self.indent(first);
        let new = self.place(at, &own, &own, to, to_rel).2;
        let shift = self.text_shift(at, &own, &new, number);
        let mut line = first;
        loop {
            let (at, blank) = self.indent(line);
            let rest = &text[at..end.max(at)];
            let blank_line = rest
                .trim_start_matches([' ', '\t'])
                .starts_with(['\n', '\r'])
                || at + blank.len() >= end;
            if !blank_line {
                let goal = if line == first {
                    to_rel
                } else {
                    (self.rel(at, &blank) as isize + shift).max(0) as usize
                };
                let (range, new, _) = self.place(at, &blank, &own, to, goal);
                if text[range.clone()] != new {
                    changes.push(Change { range, text: new });
                }
            }
            match next_line(text, line) {
                Some(next) if next < end => line = next,
                _ => break,
            }
        }
    }
}
