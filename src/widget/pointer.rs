//! What a press, a drag or the pointer is on: the source offset under a
//! point (a grid's cell, the properties), a link or `#tag` under it, and
//! clicks and drags by characters, words or lines (REFERENCE-001 section
//! 14).
use std::ops::Range;

use iced::Point;

use super::{Editor, Pane};
use crate::doc::Selection;
use crate::edit;
use crate::layout::Affinity;
use crate::style::Styled;

/// A mouse press being held.
#[derive(Debug, Clone)]
pub(super) struct Press {
    /// The selection whose reveal state stays drawn until the release, so
    /// text does not move under the pointer (REFERENCE-001 section 2).
    pub(super) frozen: Selection,
    /// What a drag extends by (REFERENCE-001 section 14).
    pub(super) unit: Unit,
    /// The word or line the press selected first.
    pub(super) first: Range<usize>,
    /// The first line drawn and its offset when the press began.
    pub(super) view: (usize, f32),
}

/// What a drag extends by: characters after a single click, words after a
/// double click, lines after a triple click.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Unit {
    Char,
    Word,
    Line,
}

impl Editor {
    /// Where the link under `at` in `pane` goes, if one is there.
    pub(super) fn link_under(&self, pane: Pane, at: Point) -> Option<String> {
        self.on_text(pane, at, Styled::link_span_at)
    }

    /// The name of the `#tag` under `at` in `pane`, if one is there.
    pub(super) fn tag_under(&self, pane: Pane, at: Point) -> Option<String> {
        self.on_text(pane, at, Styled::tag_at)
    }

    /// What `find` gives at the offset under `at` in `pane`, when the
    /// pointer is on its text, not in the space beside it (which hits its
    /// end too).
    fn on_text<'a>(
        &'a self,
        pane: Pane,
        at: Point,
        find: impl FnOnce(&'a Styled, usize) -> Option<(Range<usize>, &'a str)>,
    ) -> Option<String> {
        let (offset, _) = self.hit(pane, at);
        let (range, found) = find(&self.styled, offset)?;
        let over = self.with_pane(pane, |lines, source| {
            lines.covers(source, range, at.x, at.y)
        });
        over.then(|| found.to_owned())
    }

    /// The source offset under `at` in `pane`: in a table drawn as a grid,
    /// the cell's (REFERENCE-001 section 10).
    pub(super) fn hit(&self, pane: Pane, at: Point) -> (usize, Affinity) {
        self.with_pane(pane, |lines, source| {
            // On the properties: into the YAML, which then shows.
            if let Some((_, offset)) = lines.block_hit(source, at.x, at.y) {
                return (offset, Affinity::After);
            }
            match lines.table_hit(source, at.x, at.y) {
                Some(offset) => (offset, Affinity::After),
                None => lines.hit(source, at.x, at.y),
            }
        })
    }

    /// A press: a caret (Shift extends the selection), a word on a double
    /// click, a line on a triple click (REFERENCE-001 section 14).
    /// Returns the side the caret is drawn on, or `None` when a checkbox
    /// was ticked and the caret stayed.
    pub(super) fn press(
        &mut self,
        at: Point,
        shift: bool,
        clicks: u8,
        plain: bool,
    ) -> Option<Affinity> {
        // A tag's cross on the properties takes it out, the caret staying;
        // "+ tag" puts the caret where a new one goes (PLAN-005).
        let control = (plain && clicks == 1)
            .then(|| self.with_lines(|lines, source| lines.block_hit(source, at.x, at.y)))
            .flatten()
            .and_then(|(control, _)| control);
        let now = self.started.elapsed();
        match control {
            Some(super::properties::Hit::Remove(chip)) => {
                edit::properties::remove_tag(&mut self.doc, &self.styled, &chip, now);
                return None;
            }
            Some(super::properties::Hit::Add) => {
                edit::properties::add_tag(&mut self.doc, &self.styled, now);
                return Some(Affinity::After);
            }
            None => {}
        }
        // A checkbox toggles its task and leaves the caret where it is
        // (REFERENCE-001 section 8), each click of a quick pair too; with a
        // modifier it is an ordinary click.
        if plain {
            let task = self.with_lines(|lines, source| lines.task_at(source, at.x, at.y));
            if let Some(task) = task {
                edit::format::toggle_task(&mut self.doc, task, self.started.elapsed());
                return None;
            }
        }
        // A second or third click hits the text as the first one saw it:
        // its release revealed markers, and may have scrolled to keep the
        // caret in view (REFERENCE-001 section 2).
        let view = self.with_lines(|lines, _| (lines.anchor, lines.offset));
        let (frozen, view) = match self.last_press.take() {
            Some((last, version)) if clicks >= 2 && version == self.doc.version() => {
                self.with_lines(|lines, _| (lines.anchor, lines.offset) = last.view);
                (last.frozen, last.view)
            }
            _ => (self.doc.selection(), view),
        };
        self.press = Some(Press {
            frozen,
            unit: Unit::Char,
            first: 0..0,
            view,
        });
        let (offset, side) = self.hit(Pane::Text, at);
        // On a bullet's dot or a quote marker, drawn: the item's or quote's
        // text, so the dot stays (REFERENCE-001 sections 6, 7, 14). Drags,
        // Shift+clicks and the margin keep the line's start.
        let on_mark = clicks == 1 && plain;
        let (offset, side) = match on_mark
            .then(|| self.with_lines(|lines, source| lines.text_after_mark(source, at.x, at.y)))
            .flatten()
        {
            Some(text) => (text, Affinity::After),
            None => (offset, side),
        };
        let (unit, first) = match clicks {
            1 => (Unit::Char, offset..offset),
            2 => (
                Unit::Word,
                edit::word_at(&self.doc, offset, side == Affinity::Before),
            ),
            _ => (Unit::Line, edit::line_with_ending(&self.doc, offset)),
        };
        let selection = match unit {
            Unit::Char if shift => Selection {
                anchor: frozen.anchor,
                head: offset,
            },
            Unit::Char => Selection::caret(offset),
            Unit::Word | Unit::Line => Selection {
                anchor: first.start,
                head: first.end,
            },
        };
        self.doc.set_selection(selection);
        self.press = Some(Press {
            frozen,
            unit,
            first,
            view,
        });
        Some(side)
    }

    /// A drag: the selection grows from the press by characters, or by
    /// whole words or lines after a double or triple click, towards the
    /// pointer (CodeMirror's `basicMouseSelection`).
    pub(super) fn drag(&mut self, at: Point) -> Affinity {
        let (offset, side) = self.hit(Pane::Text, at);
        let Some(press) = &self.press else {
            return side;
        };
        let selection = match press.unit {
            Unit::Char => Selection {
                anchor: self.doc.selection().anchor,
                head: offset,
            },
            Unit::Word | Unit::Line => {
                let here = if press.unit == Unit::Word {
                    edit::word_at(&self.doc, offset, side == Affinity::Before)
                } else {
                    edit::line_with_ending(&self.doc, offset)
                };
                let from = press.first.start.min(here.start);
                let to = press.first.end.max(here.end);
                if from < here.start {
                    Selection {
                        anchor: from,
                        head: to,
                    }
                } else {
                    Selection {
                        anchor: to,
                        head: from,
                    }
                }
            }
        };
        self.doc.set_selection(selection);
        side
    }
}
