//! The document: the markdown source as one `String` (the truth, PLAN-001
//! contract 2), its line starts, the selection, and undo history grouped
//! into typing bursts (REFERENCE-001 section 15).
//!
//! One `String` and not a rope: the parser reads the whole text as one
//! `&str` after every edit, and an insert in the middle of 1 MB costs
//! 2.7 µs (PLAN-001 decisions log).
use std::ops::Range;
use std::time::Duration;

/// Edits closer together than this join one undo step (CodeMirror's
/// `newGroupDelay`, REFERENCE-001 section 15).
pub const BURST: Duration = Duration::from_millis(500);

/// `range` (byte offsets into the text before the transaction, on char
/// boundaries) replaced by `text`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Change {
    /// The bytes replaced.
    pub range: Range<usize>,
    /// What replaces them.
    pub text: String,
}

impl Change {
    /// `text` inserted at `at`.
    pub fn insert(at: usize, text: impl Into<String>) -> Self {
        Self {
            range: at..at,
            text: text.into(),
        }
    }

    /// `range` removed.
    pub fn delete(range: Range<usize>) -> Self {
        Self {
            range,
            text: String::new(),
        }
    }
}

/// The caret and selection as byte offsets: `head` moves, `anchor` stays,
/// and they are equal when nothing is selected.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Selection {
    /// Where the selection started.
    pub anchor: usize,
    /// Where the caret is.
    pub head: usize,
}

impl Selection {
    /// A caret at `at` with nothing selected.
    pub fn caret(at: usize) -> Self {
        Self {
            anchor: at,
            head: at,
        }
    }

    /// The selected bytes, in order.
    pub fn range(&self) -> Range<usize> {
        self.anchor.min(self.head)..self.anchor.max(self.head)
    }
}

/// What a transaction is to undo grouping (CodeMirror's user events,
/// REFERENCE-001 section 15).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// Typed text, IME commits included.
    Type,
    /// Backspace and Delete.
    Delete,
    /// Anything else (paste, Enter with markup, formatting, replace): it
    /// never joins the step before it.
    Other,
}

/// One transaction as history keeps it: `undo` turns the text after it
/// back into the text before it, `redo` the other way.
#[derive(Debug)]
struct Step {
    undo: Vec<Change>,
    redo: Vec<Change>,
}

/// One undo step: a transaction, or a typing burst of several.
#[derive(Debug)]
struct Event {
    steps: Vec<Step>,
    before: Selection,
    after: Selection,
    version_before: u64,
    version_after: u64,
}

/// The markdown source being edited.
#[derive(Debug)]
pub struct Doc {
    text: String,
    /// Byte offset where each line starts; the first is 0.
    lines: Vec<usize>,
    selection: Selection,
    done: Vec<Event>,
    undone: Vec<Event>,
    /// When the last change was applied; `None` once the caret moved or
    /// an undo or redo ran, which ends a burst.
    last_change: Option<Duration>,
    version: u64,
    /// The last version handed out.
    issued: u64,
}

impl Doc {
    /// A document holding `text` exactly, with the caret at the start.
    pub fn new(text: String) -> Self {
        let lines = std::iter::once(0)
            .chain(line_starts(text.as_bytes(), 1..=text.len()))
            .collect();
        Self {
            text,
            lines,
            selection: Selection::default(),
            done: Vec::new(),
            undone: Vec::new(),
            last_change: None,
            version: 0,
            issued: 0,
        }
    }

    /// The source, byte for byte.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// The caret and selection.
    pub fn selection(&self) -> Selection {
        self.selection
    }

    /// A number that changes on every edit and comes back when edits are
    /// undone or redone to the same text, for autosave: keep the value
    /// from the last save and compare.
    pub fn version(&self) -> u64 {
        self.version
    }

    /// Moves the caret or selection without editing. A move ends a typing
    /// burst.
    ///
    /// # Panics
    ///
    /// When an end is past the text or not on a char boundary.
    pub fn set_selection(&mut self, selection: Selection) {
        self.check(selection);
        if selection != self.selection {
            self.selection = selection;
            self.last_change = None;
        }
    }

    /// Applies `changes` (sorted, not overlapping, in the coordinates of
    /// the current text) as one transaction made at `now`, leaving
    /// `selection` (in the coordinates of the new text). Typing and
    /// deleting join the previous undo step when it was changed less than
    /// [`BURST`] ago, next to these changes, with no caret move between
    /// (REFERENCE-001 section 15).
    ///
    /// # Panics
    ///
    /// When the changes are out of order, overlap, or cut a character, or
    /// the selection does not fit the new text.
    pub fn apply(&mut self, changes: Vec<Change>, selection: Selection, kind: Kind, now: Duration) {
        if changes.is_empty() {
            self.set_selection(selection);
            return;
        }
        let mut from = 0;
        for change in &changes {
            let Range { start, end } = change.range;
            assert!(
                from <= start
                    && start <= end
                    && self.text.is_char_boundary(start)
                    && self.text.is_char_boundary(end),
                "change {:?} out of order or not on char boundaries",
                change.range,
            );
            from = end;
        }
        let mut delta = 0isize;
        let undo = changes
            .iter()
            .map(|change| {
                let start = change.range.start.wrapping_add_signed(delta);
                delta += change.text.len() as isize - change.range.len() as isize;
                Change {
                    range: start..start + change.text.len(),
                    text: self.text[change.range.clone()].to_owned(),
                }
            })
            .collect::<Vec<_>>();
        let joins = matches!(kind, Kind::Type | Kind::Delete)
            && self
                .last_change
                .is_some_and(|last| now.saturating_sub(last) < BURST)
            && self.done.last().is_some_and(|event| {
                let last = &event.steps[event.steps.len() - 1].undo;
                last.iter().any(|a| {
                    changes
                        .iter()
                        .any(|b| b.range.start <= a.range.end && a.range.start <= b.range.end)
                })
            });
        self.replace(&changes);
        self.check(selection);
        self.issued += 1;
        let step = Step {
            undo,
            redo: changes,
        };
        match self.done.last_mut() {
            Some(event) if joins => {
                event.steps.push(step);
                event.after = selection;
                event.version_after = self.issued;
            }
            _ => self.done.push(Event {
                steps: vec![step],
                before: self.selection,
                after: selection,
                version_before: self.version,
                version_after: self.issued,
            }),
        }
        self.undone.clear();
        self.version = self.issued;
        self.selection = selection;
        self.last_change = Some(now);
    }

    /// Undoes the last undo step and puts back the selection from before
    /// it. Returns whether there was one.
    pub fn undo(&mut self) -> bool {
        let Some(event) = self.done.pop() else {
            return false;
        };
        for step in event.steps.iter().rev() {
            self.replace(&step.undo);
        }
        self.selection = event.before;
        self.version = event.version_before;
        self.last_change = None;
        self.undone.push(event);
        true
    }

    /// Redoes the last undone step and puts back the selection from after
    /// it. Returns whether there was one.
    pub fn redo(&mut self) -> bool {
        let Some(event) = self.undone.pop() else {
            return false;
        };
        for step in &event.steps {
            self.replace(&step.redo);
        }
        self.selection = event.after;
        self.version = event.version_after;
        self.last_change = None;
        self.done.push(event);
        true
    }

    /// How many lines there are: one more than line endings (`\n`, `\r\n`
    /// or a lone `\r`, as in CommonMark).
    pub fn line_count(&self) -> usize {
        self.lines.len()
    }

    /// The line holding `offset`; a line ending belongs to its line.
    pub fn line_at(&self, offset: usize) -> usize {
        self.lines.partition_point(|&start| start <= offset) - 1
    }

    /// The bytes of line `index`, without its line ending.
    pub fn line_range(&self, index: usize) -> Range<usize> {
        let start = self.lines[index];
        let end = self
            .lines
            .get(index + 1)
            .copied()
            .unwrap_or(self.text.len());
        let line = &self.text.as_bytes()[start..end];
        let ending = match line {
            [.., b'\r', b'\n'] => 2,
            [.., b'\n' | b'\r'] => 1,
            _ => 0,
        };
        start..end - ending
    }

    /// Applies sorted, non-overlapping changes from the last one back, so
    /// each range is still where it was, and keeps the line starts.
    fn replace(&mut self, changes: &[Change]) {
        for change in changes.iter().rev() {
            let Range { start, end } = change.range;
            self.text.replace_range(start..end, &change.text);
            self.reindex(start, end, start + change.text.len());
        }
    }

    /// Updates the line starts after `start..old_end` became
    /// `start..new_end`. Whether `p` starts a line depends on the bytes at
    /// `p - 1` and `p` (a `\r` before a `\n` ends no line), so starts
    /// before `start` stay, starts after `old_end + 1` shift, and the ones
    /// between are found again.
    fn reindex(&mut self, start: usize, old_end: usize, new_end: usize) {
        let first = start.max(1);
        let keep = self.lines.partition_point(|&s| s < first);
        let tail = self.lines.partition_point(|&s| s <= old_end + 1);
        let delta = new_end as isize - old_end as isize;
        for s in &mut self.lines[tail..] {
            *s = s.wrapping_add_signed(delta);
        }
        let found: Vec<usize> = line_starts(
            self.text.as_bytes(),
            first..=(new_end + 1).min(self.text.len()),
        )
        .collect();
        self.lines.splice(keep..tail, found);
    }

    fn check(&self, selection: Selection) {
        for end in [selection.anchor, selection.head] {
            assert!(
                self.text.is_char_boundary(end),
                "selection end {end} past the text or inside a character"
            );
        }
    }
}

/// The offsets in `range` (all at least 1) where a line starts: after a
/// `\n`, or after a `\r` that no `\n` follows.
fn line_starts(text: &[u8], range: std::ops::RangeInclusive<usize>) -> impl Iterator<Item = usize> {
    range.filter(move |&p| match text[p - 1] {
        b'\n' => true,
        b'\r' => text.get(p) != Some(&b'\n'),
        _ => false,
    })
}

#[cfg(test)]
mod tests;
