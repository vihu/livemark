//! What the editor does with its view's messages: clicks and drags, keys,
//! input method commits and the clipboard (REFERENCE-001 sections 13 to 19).
use iced::{Point, Task};

use super::{Editor, Input, Key, Message, Mode, Press, Unit, Vertical};
use crate::doc::Selection;
use crate::edit::{self, Motion};
use crate::layout::Affinity;
use crate::style::Styled;

impl Editor {
    /// Applies a message from [`Editor::view`]. The task writes the
    /// clipboard after a copy or cut; run it, or copying does nothing.
    pub fn update(&mut self, Message(input): Message) -> Task<Message> {
        // A toolbar button: its key's command, and the keyboard back to the
        // text (the button took the press).
        if let Input::Tool(key) = input {
            let task = self.update(Message(Input::Key(key)));
            return Task::batch([task, iced::widget::operation::focus(super::ID)]);
        }
        let now = self.started.elapsed();
        let version = self.doc.version();
        let mut side = Affinity::After;
        let mut vertical = false;
        let mut task = Task::none();
        // Inputs that move nothing keep the view where it is, even with the
        // caret off screen: following a link, ticking a checkbox, copying,
        // selecting all.
        let mut keep_view = false;
        // Any input but scrolling places the caret (REFERENCE-001
        // section 2); a press only on release, so the text clicked does
        // not move under the pointer.
        self.placed |= !matches!(
            input,
            Input::Scroll(_)
                | Input::ScrollTo(_)
                | Input::ZoomSteps(_)
                | Input::ZoomBy(_)
                | Input::Shift(_)
                | Input::Press { .. }
                | Input::Drag(_)
        );
        match input {
            Input::Press {
                at,
                command: true,
                clicks,
                ..
            } if self.link_under(at).is_some() => {
                // Ctrl/Cmd+click on a link follows it and moves nothing
                // (REFERENCE-001 section 5); a quick second one does
                // nothing more.
                if clicks == 1 {
                    let dest = self.link_under(at).unwrap_or_default();
                    task = Task::done(Message(Input::Follow(dest)));
                }
                side = self.side;
                keep_view = true;
            }
            Input::Press {
                at,
                shift,
                clicks,
                command,
                other,
            } => match self.press(at, shift, clicks, !(shift || command || other)) {
                Some(placed) => side = placed,
                None => {
                    side = self.side;
                    keep_view = true;
                }
            },
            Input::Follow(_) => {
                side = self.side;
                keep_view = true;
            }
            // The pointer moving after a tick or a link click, which start
            // no press: nothing moves.
            Input::Drag(_) if self.press.is_none() => {
                side = self.side;
                keep_view = true;
            }
            Input::Drag(at) => side = self.drag(at),
            // The end of a press shows the markers it froze, which can move
            // the caret out of view: it is brought back, unless it was out
            // of view already (scrolled away while the button was held).
            Input::Release => {
                let height = self.lines.borrow().height;
                let shown = self
                    .caret()
                    .is_some_and(|caret| caret.y >= 0.0 && caret.y + caret.height <= height);
                let press = self.press.take();
                keep_view = press.is_none() || !shown;
                self.last_press = press.map(|press| (press, self.doc.version()));
                side = self.side;
            }
            Input::Scroll(dy) => {
                self.with_lines(|lines, source| lines.scroll_by(source, dy));
                return Task::none();
            }
            Input::ScrollTo(t) => {
                self.scroll_to(t);
                return Task::none();
            }
            Input::ZoomSteps(steps) => {
                self.set_zoom(((self.zoom() * 10.0).round() + steps) / 10.0);
                return Task::none();
            }
            Input::ZoomBy(factor) => {
                self.set_zoom(self.zoom() * factor);
                return Task::none();
            }
            Input::Shift(held) => {
                self.shift = held;
                return Task::none();
            }
            Input::Commit(text) => edit::type_text(&mut self.doc, &text, now),
            // Handled before this match.
            Input::Tool(_) => {}
            Input::Find(input) => {
                // Typing a replacement, closing the bar or a query with no
                // match moves nothing: the view stays.
                let before = (self.doc.selection(), self.doc.version());
                task = self.find_input(input, now);
                keep_view = before == (self.doc.selection(), self.doc.version());
            }
            Input::Paste(text) => {
                let line = self.doc.selection().range().is_empty()
                    && self.linewise.as_deref() == Some(text.as_str());
                if line {
                    edit::paste_line(&mut self.doc, &text, now);
                } else {
                    edit::paste(&mut self.doc, &text, now);
                }
            }
            Input::Key(key) => match key {
                Key::Insert(c) => edit::type_text(&mut self.doc, c.encode_utf8(&mut [0; 4]), now),
                Key::Enter => edit::enter(&mut self.doc, now),
                Key::SoftEnter => edit::markup::soft_break(&mut self.doc, now),
                Key::Indent(outdent) => edit::markup::indent(&mut self.doc, outdent, now),
                Key::Format(format) => {
                    edit::format::toggle(&mut self.doc, &self.styled, format, now);
                }
                Key::Link => edit::format::link(&mut self.doc, now),
                Key::Block(block) => edit::blocks::toggle(&mut self.doc, block, now),
                Key::SetMode(mode) => self.set_mode(mode),
                Key::MoveLines(down) => edit::lines::move_lines(&mut self.doc, down, now),
                Key::CopyLines(down) => edit::lines::copy_lines(&mut self.doc, down, now),
                Key::DeleteLines => edit::lines::delete_lines(&mut self.doc, now),
                Key::BlankLine => edit::lines::blank_line(&mut self.doc, now),
                Key::Follow => {
                    if let Some(dest) = self.styled.link_at(self.doc.selection().head) {
                        task = Task::done(Message(Input::Follow(dest.to_owned())));
                    }
                }
                Key::ToggleMode => self.set_mode(match self.mode {
                    Mode::Live => Mode::Source,
                    Mode::Source => Mode::Live,
                }),
                Key::Delete(Motion::Left) => edit::backspace(&mut self.doc, now),
                Key::Delete(motion) => edit::delete(&mut self.doc, motion, now),
                Key::Move(motion @ (Motion::LineStart | Motion::LineEnd), extend) => {
                    side = self.line_boundary(motion == Motion::LineEnd, extend);
                }
                Key::Move(motion, extend) => edit::go(&mut self.doc, motion, extend),
                Key::Vertical(direction, extend) => {
                    side = self.vertical(direction, extend);
                    vertical = true;
                }
                Key::Copy(cut) => {
                    // With nothing selected, the line (REFERENCE-001
                    // section 18); worked out here, after every earlier
                    // message, so a click just before is counted.
                    let (text, _, linewise) = edit::copied(&self.doc);
                    self.linewise = linewise.then(|| text.clone());
                    if cut {
                        edit::cut(&mut self.doc, now);
                    } else {
                        keep_view = true;
                    }
                    task = iced::clipboard::write(text).discard();
                }
                Key::SelectAll => {
                    self.doc.set_selection(Selection {
                        anchor: 0,
                        head: self.doc.text().len(),
                    });
                    keep_view = true;
                }
                Key::Undo => {
                    self.doc.undo();
                }
                Key::Redo => {
                    self.doc.redo();
                }
                Key::Collapse => {
                    let head = self.doc.selection().head;
                    self.doc.set_selection(Selection::caret(head));
                }
            },
        }
        if self.doc.version() != version {
            self.styled = Styled::new(self.doc.text());
            // An edit while the button is held (a key, a paste) ends the
            // press: its word or line is from the old text.
            self.press = None;
        }
        if !vertical {
            self.goal_x = None;
        }
        self.side = side;
        let head = self.doc.selection().head;
        if !keep_view {
            self.with_lines(|lines, source| lines.reveal(source, head, side));
        }
        self.refresh_matches();
        task
    }

    /// Where the link under `at` goes, if one is there.
    pub(super) fn link_under(&self, at: Point) -> Option<String> {
        let (offset, _) = self.hit(at);
        let (range, dest) = self.styled.link_span_at(offset)?;
        // On the link's text, not in the space beside it, which hits its
        // end too.
        let over = self.with_lines(|lines, source| lines.covers(source, range, at.x, at.y));
        over.then(|| dest.to_owned())
    }

    /// The source offset under `at`: in a table drawn as a grid, the
    /// cell's (REFERENCE-001 section 10).
    fn hit(&self, at: Point) -> (usize, Affinity) {
        self.with_lines(|lines, source| match lines.table_hit(source, at.x, at.y) {
            Some(offset) => (offset, Affinity::After),
            None => lines.hit(source, at.x, at.y),
        })
    }

    /// A press: a caret (Shift extends the selection), a word on a double
    /// click, a line on a triple click (REFERENCE-001 section 14).
    /// Returns the side the caret is drawn on, or `None` when a checkbox
    /// was ticked and the caret stayed.
    fn press(&mut self, at: Point, shift: bool, clicks: u8, plain: bool) -> Option<Affinity> {
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
        let (offset, side) = self.hit(at);
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
    fn drag(&mut self, at: Point) -> Affinity {
        let (offset, side) = self.hit(at);
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

    /// Home and End: the edge of the visual row, then of the source line,
    /// Home stopping at the indentation (REFERENCE-001 section 13). The
    /// caret stays on the row it moved to the edge of.
    fn line_boundary(&mut self, forward: bool, extend: bool) -> Affinity {
        let selection = self.doc.selection();
        let side = self.side;
        let row = self.with_lines(|lines, source| lines.row_bounds(source, selection.head, side));
        let line = self.doc.line_range(self.doc.line_at(selection.head));
        let markup = self.styled.hang_at(line);
        let head = edit::line_boundary(&self.doc, selection.head, row, forward, markup);
        let anchor = if extend { selection.anchor } else { head };
        self.doc.set_selection(Selection { anchor, head });
        if forward {
            Affinity::Before
        } else {
            Affinity::After
        }
    }

    /// Moves the caret a row or a page up or down, aiming for the same x.
    fn vertical(&mut self, direction: Vertical, extend: bool) -> Affinity {
        let selection = self.doc.selection();
        let (goal, side) = (self.goal_x, self.side);
        let (x, (head, side)) = self.with_lines(|lines, source| {
            let index = source.doc.line_at(selection.head);
            // From the caret, so first to it when it is far out of view.
            if lines.top_of(source, index).is_none() {
                lines.reveal(source, selection.head, side);
            }
            let top = lines.top_of(source, index).unwrap_or(0.0);
            let (x, row_top, row_height) = lines.caret_in_line(source, selection.head, side);
            let x = goal.unwrap_or(x);
            let row = top + row_top;
            // Past a line's pictures, to text rows: down from its last row
            // to the next line, up from a line to the one above's last row.
            let shaped = lines.shaped(source, index);
            let last_row = row_top + row_height >= shaped.text_bottom() - 0.5;
            let above = index.checked_sub(1).map(|i| {
                (
                    top - lines.shaped(source, i).height,
                    lines.shaped(source, i),
                )
            });
            let y = match direction {
                Vertical::Up if row_top < 0.5 => above.map_or(row - 1.0, |(above_top, shaped)| {
                    above_top + shaped.text_bottom() - 1.0
                }),
                Vertical::Up => row - 1.0,
                Vertical::Down if last_row => top + shaped.height + 1.0,
                Vertical::Down => row + row_height + 1.0,
                Vertical::PageUp => row - lines.height,
                Vertical::PageDown => row + lines.height,
            };
            (x, lines.hit(source, x, y))
        });
        self.goal_x = Some(x);
        let anchor = if extend { selection.anchor } else { head };
        self.doc.set_selection(Selection { anchor, head });
        side
    }
}
