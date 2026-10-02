//! Completion the host fills (PLAN-004, PLAN-001 L3b part 2): after `[[`
//! (a link) or a `#` starting a tag, the editor says what is being typed
//! ([`Editor::completing`]); the host answers with choices
//! ([`Editor::set_choices`]), shown in a list under the caret. Up and
//! Down choose, Enter or Tab (or a click) puts the choice's text in place
//! of what was typed, the trigger included; Escape closes it until the
//! next trigger.
use std::ops::Range;

use iced::widget::{button, column, container, pin, text};
use iced::{Element, Length, Point, Theme};

use super::{Editor, Input, Message};
use crate::doc::{Change, Kind, Selection};

/// What a completion is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Complete {
    /// After `[[`: a link to another note.
    Link,
    /// After `#` at a word's start: a tag.
    Tag,
}

/// What is being completed: the kind and what was typed after its
/// trigger.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Completing {
    /// A link or a tag.
    pub kind: Complete,
    /// What was typed after `[[` or `#`.
    pub query: String,
}

/// A choice the host offers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Choice {
    /// What the list shows.
    pub label: String,
    /// A second, quieter line (a date, a count); may be empty.
    pub detail: String,
    /// What replaces the trigger and the text typed after it.
    pub insert: String,
}

/// A completion under way.
#[derive(Debug, Default)]
pub(super) struct Completion {
    kind: Option<Complete>,
    /// Where its trigger starts, and the caret.
    range: Range<usize>,
    choices: Vec<Choice>,
    selected: usize,
    /// Escape closed it: shown again only for another trigger.
    dismissed: bool,
}

/// The most choices listed.
const SHOWN: usize = 8;

/// The list's width.
const WIDTH: f32 = 380.0;

#[derive(Debug, Clone)]
pub(super) enum CompleteInput {
    /// Up (-1) or Down (1).
    Move(i32),
    /// Enter or Tab (the chosen one), or a click (this one).
    Accept(Option<usize>),
    Dismiss,
}

// Public API
impl Editor {
    /// What is being typed after a trigger at the caret (`[[` for a link,
    /// `#` for a tag), for the host to answer with [`Editor::set_choices`];
    /// `None` when nothing is, or Escape closed it.
    pub fn completing(&self) -> Option<Completing> {
        let completion = &self.completion;
        let kind = completion.kind.filter(|_| !completion.dismissed)?;
        let typed = &self.doc.text()[completion.range.clone()];
        let trigger = match kind {
            Complete::Link => 2,
            Complete::Tag => 1,
        };
        Some(Completing {
            kind,
            query: typed[trigger..].to_owned(),
        })
    }

    /// The host's choices for what is being completed, best first; none
    /// hides the list.
    pub fn set_choices(&mut self, choices: Vec<Choice>) {
        self.completion.choices = choices;
        self.completion.choices.truncate(SHOWN);
        self.completion.selected = 0;
    }
}

// Private API
impl Editor {
    /// Whether the list is up, taking Up, Down, Enter, Tab and Escape.
    pub(super) fn choosing(&self) -> bool {
        self.completion.kind.is_some()
            && !self.completion.dismissed
            && !self.completion.choices.is_empty()
    }

    /// Finds the trigger at the caret again after an input: the same one
    /// keeps its choices and Escape; another starts over.
    pub(super) fn refresh_completion(&mut self) {
        let found = self.trigger();
        let completion = &mut self.completion;
        match found {
            Some((kind, range))
                if completion.kind == Some(kind) && completion.range.start == range.start =>
            {
                completion.range = range;
            }
            Some((kind, range)) => {
                *completion = Completion {
                    kind: Some(kind),
                    range,
                    ..Completion::default()
                };
            }
            None => *completion = Completion::default(),
        }
    }

    /// `[[` or a tag's `#` before the caret, on its line, with no closing
    /// `]` or anything not a tag's after it; never in code.
    fn trigger(&self) -> Option<(Complete, Range<usize>)> {
        let selection = self.doc.selection();
        if !selection.range().is_empty() {
            return None;
        }
        let head = selection.head;
        let text = self.doc.text();
        let line_start = text[..head].rfind(['\n', '\r']).map_or(0, |i| i + 1);
        let before = &text[line_start..head];
        if self.in_code(head) {
            return None;
        }
        if let Some(at) = before.rfind("[[")
            && !before[at + 2..].contains(']')
        {
            return Some((Complete::Link, line_start + at..head));
        }
        let name = |c: char| c.is_alphanumeric() || matches!(c, '_' | '-' | '/');
        let start = before
            .char_indices()
            .rev()
            .take_while(|&(_, c)| name(c))
            .last()
            .map_or(before.len(), |(i, _)| i);
        // At least one letter typed: a lone `#` may be a heading.
        if !before[..start].ends_with('#') {
            return None;
        }
        let hash = start - 1;
        (start < before.len() && crate::parse::tag_at(text, line_start + hash).is_some())
            .then(|| (Complete::Tag, line_start + hash..head))
    }

    /// Whether `offset` is in inline code or a code block.
    fn in_code(&self, offset: usize) -> bool {
        let runs = self.styled.runs();
        let at = offset.saturating_sub(1);
        let i = runs.partition_point(|(r, _)| r.end <= at);
        runs.get(i)
            .is_some_and(|(r, style)| r.start <= at && (style.code || style.code_block))
    }

    /// Carries out a key or click on the list.
    pub(super) fn complete_input(&mut self, input: CompleteInput) {
        match input {
            CompleteInput::Move(step) => {
                let last = self.completion.choices.len().saturating_sub(1) as i32;
                let selected = self.completion.selected as i32 + step;
                self.completion.selected = selected.clamp(0, last) as usize;
            }
            CompleteInput::Dismiss => self.completion.dismissed = true,
            CompleteInput::Accept(at) => {
                let at = at.unwrap_or(self.completion.selected);
                let Some(choice) = self.completion.choices.get(at).cloned() else {
                    return;
                };
                let range = self.completion.range.clone();
                let caret = range.start + choice.insert.len();
                self.doc.apply(
                    vec![Change {
                        range: range.clone(),
                        text: choice.insert,
                    }],
                    Selection::caret(caret),
                    Kind::Other,
                    self.started.elapsed(),
                );
                // A tag put in is a trigger again: closed for it.
                self.completion = Completion {
                    kind: self.completion.kind,
                    range: range.start..caret,
                    dismissed: true,
                    ..Completion::default()
                };
            }
        }
    }

    /// The list under the caret, placed in the area the text widget fills.
    pub(super) fn completion_view(&self) -> Option<Element<'_, Message>> {
        if !self.choosing() {
            return None;
        }
        let caret = self.caret()?;
        let (room, height) = {
            let lines = self.lines.borrow();
            // Placed from the text's size: none before the first layout.
            if !lines.sized {
                return None;
            }
            (lines.outer.width - lines.width, lines.height)
        };
        // Where the text area starts in the widget: the padding and
        // gutter left of it (all of `room` but the right padding).
        let left = room - super::surface::PADDING;
        let top = super::surface::PADDING;
        let rows = self.completion.choices.len() as f32;
        let tall = rows * 44.0 + 8.0;
        let below = caret.y + caret.height + 4.0;
        let y = if below + tall > height + top && caret.y > tall {
            caret.y - tall - 4.0
        } else {
            below
        };
        let mut list = column![].spacing(1);
        for (i, choice) in self.completion.choices.iter().enumerate() {
            let on = i == self.completion.selected;
            let entry = column![text(choice.label.as_str()).size(14)].push(
                (!choice.detail.is_empty()).then(|| {
                    text(choice.detail.as_str())
                        .size(11)
                        .style(move |theme: &Theme| text::Style {
                            color: Some(row_text(theme, on).scale_alpha(0.6)),
                        })
                }),
            );
            list = list.push(
                button(entry)
                    .width(Length::Fill)
                    .padding([4, 10])
                    .style(move |theme: &Theme, status| {
                        let palette = theme.palette();
                        let background = match status {
                            _ if on => Some(palette.primary.weak.color),
                            button::Status::Hovered => Some(palette.background.weak.color),
                            _ => None,
                        };
                        button::Style {
                            background: background.map(iced::Background::Color),
                            text_color: row_text(theme, on),
                            border: iced::Border::default().rounded(5),
                            ..button::Style::default()
                        }
                    })
                    .on_press(Message(Input::Complete(CompleteInput::Accept(Some(i))))),
            );
        }
        let panel = container(list)
            .width(WIDTH)
            .padding(4)
            .style(container::bordered_box);
        Some(
            pin(panel)
                .position(Point::new(left + caret.x, top + y))
                .into(),
        )
    }
}

fn row_text(theme: &Theme, on: bool) -> iced::Color {
    let palette = theme.palette();
    if on {
        palette.primary.weak.text
    } else {
        palette.background.base.text
    }
}

#[cfg(test)]
mod tests {
    use super::{Choice, Complete, CompleteInput, Completing};
    use crate::widget::{Editor, Input, Message};

    fn typed(text: &str) -> Editor {
        let mut editor = Editor::new(String::new());
        let _ = editor.update(Message(Input::Commit(text.into())));
        editor
    }

    fn choice(label: &str, insert: &str) -> Choice {
        Choice {
            label: label.into(),
            detail: String::new(),
            insert: insert.into(),
        }
    }

    fn send(editor: &mut Editor, input: CompleteInput) {
        let _ = editor.update(Message(Input::Complete(input)));
    }

    #[test]
    fn a_link_is_chosen_from_the_hosts_list_after_two_brackets() {
        let mut editor = typed("see [[lis");
        assert_eq!(
            editor.completing(),
            Some(Completing {
                kind: Complete::Link,
                query: "lis".into()
            })
        );
        assert!(!editor.choosing(), "no list until the host answers");
        editor.set_choices(vec![
            choice("Lisbon conference", "[Lisbon conference](b.md)"),
            choice("Lisbon hotels", "[Lisbon hotels](a.md)"),
        ]);
        assert!(editor.choosing());
        send(&mut editor, CompleteInput::Move(1));
        send(&mut editor, CompleteInput::Move(5));
        send(&mut editor, CompleteInput::Accept(None));
        assert_eq!(editor.text(), "see [Lisbon hotels](a.md)");
        assert_eq!(editor.selection().head, editor.text().len());
        assert_eq!(editor.completing(), None);
    }

    #[test]
    fn a_tag_is_completed_once_and_escape_closes_the_list() {
        let mut editor = typed("plan #tr");
        assert_eq!(editor.completing().map(|c| c.query), Some("tr".into()));
        editor.set_choices(vec![choice("#travel", "#travel")]);
        send(&mut editor, CompleteInput::Accept(None));
        assert_eq!(editor.text(), "plan #travel");
        assert_eq!(editor.completing(), None, "not again for the tag put in");
        // Escape closes it for this trigger; typing on keeps it closed.
        let mut editor = typed("a #wo");
        editor.set_choices(vec![choice("#work", "#work")]);
        send(&mut editor, CompleteInput::Dismiss);
        let _ = editor.update(Message(Input::Commit("r".into())));
        assert_eq!(editor.completing(), None);
        assert!(!editor.choosing());
    }

    #[test]
    fn headings_code_and_closed_links_trigger_nothing() {
        for text in [
            "#",
            "# ",
            "## Head",
            "`#tr",
            "a#tr",
            "[[x]",
            "x [[a]] b",
            "issue #12",
            // Found by the widget fuzz: a letter after a wide character.
            "\u{1f600}abc",
            "\u{e9}t\u{e9}",
        ] {
            let editor = typed(text);
            assert_eq!(editor.completing(), None, "{text:?}");
        }
        assert_eq!(
            typed("[[").completing().map(|c| c.query),
            Some(String::new())
        );
    }
}
