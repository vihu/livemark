//! Side by side's right pane (PLAN-003): the same document drawn as live
//! preview with every marker hidden and every mark drawn, as if the caret
//! touched nothing; not editable, with its own lines and scroll. The two
//! panes scroll together by source line: the one scrolled last leads, and
//! the other shows the same line at its top with the same share of it
//! scrolled past. A click in it puts the caret there in the markdown.
use std::cell::{Cell, RefCell};

use iced::{Point, Task};

use super::lines::Lines;
use super::shape::Colors;
use super::{Editor, ID, Input, Message, Mode, Reveal};
use crate::doc::Selection;
use crate::edit;

/// One of the two panes of side by side; the text alone otherwise.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pane {
    /// The markdown, edited.
    Text,
    /// The rendered note beside it.
    Preview,
}

/// The rendered pane's lines, what it hides, and which pane leads.
pub struct Preview {
    pub(super) lines: RefCell<Lines>,
    /// What it hides, by document version: cleared when a picture comes.
    pub(super) reveal: RefCell<Option<(u64, Reveal)>>,
    /// The pane scrolled last, which the other follows.
    pub(super) leader: Cell<Pane>,
}

impl Preview {
    pub(super) fn new(colors: Colors) -> Self {
        Self {
            lines: RefCell::new(Lines::new(colors)),
            reveal: RefCell::new(None),
            leader: Cell::new(Pane::Text),
        }
    }
}

impl Editor {
    /// What the rendered pane hides and draws over: everything, as if the
    /// caret were nowhere.
    pub(super) fn preview_reveal(&self) -> Reveal {
        let version = self.doc.version();
        let mut cache = self.preview.reveal.borrow_mut();
        match &*cache {
            Some((v, reveal)) if *v == version => reveal.clone(),
            _ => {
                let reveal = self.reveal_for(usize::MAX..usize::MAX);
                *cache = Some((version, reveal.clone()));
                reveal
            }
        }
    }

    /// Puts the pane not scrolled last where the other is: the same line
    /// at its top, the same share of it scrolled past; at the end when the
    /// leader is at its end. Only side by side, once both are laid out.
    pub(super) fn follow(&self) {
        if self.mode != Mode::Split || !self.preview.lines.borrow().sized {
            return;
        }
        let leader = self.preview.leader.get();
        let follower = match leader {
            Pane::Text => Pane::Preview,
            Pane::Preview => Pane::Text,
        };
        let (anchor, share, at_end) = self.with_pane(leader, |lines, source| {
            let height = lines.shaped(source, lines.anchor).height.max(1.0);
            let share = (lines.offset / height).clamp(0.0, 1.0);
            (lines.anchor, share, at_end(lines, source))
        });
        self.with_pane(follower, |lines, source| {
            if at_end {
                (lines.anchor, lines.offset) = lines.end(source);
                return;
            }
            lines.anchor = anchor.min(source.doc.line_count() - 1);
            lines.offset = share * lines.shaped(source, lines.anchor).height;
            lines.stop_at_end(source);
        });
    }

    /// A press in the rendered pane at `at` (in its text area): a link
    /// followed with Ctrl/Cmd, a checkbox ticked, or else the caret put at
    /// that place in the markdown, which takes the keyboard.
    pub(super) fn preview_press(&mut self, at: Point, command: bool) -> Task<Message> {
        if let Some(dest) = self.footer_under(Pane::Preview, at) {
            return Task::done(Message(Input::Follow(dest)));
        }
        if command && let Some(dest) = self.link_under(Pane::Preview, at) {
            return Task::done(Message(Input::Follow(dest)));
        }
        if command && let Some(name) = self.tag_under(Pane::Preview, at) {
            return Task::done(Message(Input::Tag(name)));
        }
        let task = self.with_pane(Pane::Preview, |lines, source| {
            lines.task_at(source, at.x, at.y)
        });
        if let Some(task) = task {
            edit::format::toggle_task(&mut self.doc, task, self.started.elapsed());
            return Task::none();
        }
        let (offset, side) = self.hit(Pane::Preview, at);
        self.doc.set_selection(Selection::caret(offset));
        self.placed = true;
        self.goal_x = None;
        self.side = side;
        // The markdown follows the rendered pane, then shows the caret,
        // leading only if that moved it.
        self.preview.leader.set(Pane::Preview);
        self.follow();
        let before = self.with_lines(|lines, _| (lines.anchor, lines.offset));
        self.with_lines(|lines, source| lines.reveal(source, offset, side));
        if self.with_lines(|lines, _| (lines.anchor, lines.offset)) != before {
            self.preview.leader.set(Pane::Text);
            self.follow();
        }
        iced::widget::operation::focus(ID)
    }
}

/// Whether the view is scrolled to the document's end, and not at its top
/// (a short document is both). Only measured near the end, as
/// `Lines::stop_at_end` does.
fn at_end(lines: &mut Lines, source: &super::lines::Source) -> bool {
    let last = source.doc.line_count() - 1;
    let far = last - lines.anchor.min(last) > (lines.height / 8.0) as usize + 1;
    if far || (lines.anchor == 0 && lines.offset == 0.0) {
        return false;
    }
    let (end, offset) = lines.end(source);
    lines.anchor > end || (lines.anchor == end && lines.offset >= offset - 0.5)
}
