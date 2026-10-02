//! The find bar under the text (REFERENCE-001 section 16): a query field
//! with the match count, case, previous, next and close, and with Ctrl/Cmd+H
//! a replacement field with replace and replace all.
use std::ops::Range;
use std::time::Duration;

use iced::advanced::widget::Id;
use iced::widget::{button, column, row, text, text_input};
use iced::{Element, Fill, Task};

use super::{Editor, ID, Input, Message};
use crate::doc::Selection;
use crate::edit::find::{self, Query, next_match};

const FIND: Id = Id::new("livemark-find");
const REPLACE: Id = Id::new("livemark-replace");

/// The longest selection that seeds the query when the bar opens.
const SEED: usize = 100;

/// The find bar's state.
#[derive(Debug, Default)]
pub(super) struct Find {
    query: Query,
    replacement: String,
    replacing: bool,
    /// Where the selection started when the bar opened: a new query selects
    /// its first match from here.
    origin: usize,
    /// The matches, and the document version and query they are for.
    pub(super) matches: Vec<Range<usize>>,
    found_for: Option<(u64, Query)>,
}

/// What the find bar or its keys ask for.
#[derive(Debug, Clone)]
pub(super) enum FindInput {
    /// Ctrl/Cmd+F, or Ctrl/Cmd+H with `replace`.
    Open {
        replace: bool,
    },
    Query(String),
    Replacement(String),
    Case(bool),
    /// F3, Ctrl/Cmd+G or Enter in the query: the next match; with Shift
    /// the previous one.
    Step {
        forward: bool,
    },
    Replace,
    ReplaceAll,
    Close,
}

impl Editor {
    pub(super) fn find_input(&mut self, input: FindInput, now: Duration) -> Task<Message> {
        if let FindInput::Open { replace } = input {
            // The selection seeds the query when it is one short line
            // (CodeMirror's `defaultQuery`).
            let range = self.doc.selection().range();
            let selected = &self.doc.text()[range.clone()];
            let seed = (!selected.is_empty()
                && selected.len() <= SEED
                && !selected.contains(['\n', '\r']))
            .then(|| selected.to_owned());
            let find = self.find.get_or_insert_with(Find::default);
            find.replacing |= replace;
            find.origin = range.start;
            if let Some(seed) = seed {
                find.query.text = seed;
            }
            // The query first, with or without the replace row (as in
            // CodeMirror's panel).
            return Task::batch([
                iced::widget::operation::focus(FIND),
                iced::widget::operation::select_all(FIND),
            ]);
        }
        let Some(state) = self.find.as_mut() else {
            return Task::none();
        };
        match input {
            FindInput::Open { .. } => {}
            FindInput::Query(text) => {
                state.query.text = text;
                self.select_first_match();
            }
            FindInput::Case(on) => {
                state.query.case_sensitive = on;
                self.select_first_match();
            }
            FindInput::Replacement(text) => state.replacement = text,
            FindInput::Step { forward } => {
                let query = state.query.clone();
                find::find(&mut self.doc, &query, forward && !self.shift);
            }
            FindInput::Replace => {
                let (query, replacement) = (state.query.clone(), state.replacement.clone());
                find::replace(&mut self.doc, &query, &replacement, now);
            }
            FindInput::ReplaceAll => {
                let (query, replacement) = (state.query.clone(), state.replacement.clone());
                find::replace_all(&mut self.doc, &query, &replacement, now);
            }
            FindInput::Close => {
                self.find = None;
                return iced::widget::operation::focus(ID);
            }
        }
        Task::none()
    }

    /// Typing a query selects its first match from where the search
    /// started, so the view follows (as Obsidian does).
    fn select_first_match(&mut self) {
        let Some(state) = &self.find else {
            return;
        };
        let matches = state.query.matches(self.doc.text());
        if let Some(found) = next_match(&matches, state.origin, state.origin) {
            self.doc.set_selection(Selection {
                anchor: found.start,
                head: found.end,
            });
        }
    }

    /// Finds the matches again when the text or the query changed.
    pub(super) fn refresh_matches(&mut self) {
        let version = self.doc.version();
        if let Some(state) = &mut self.find {
            let key = (version, state.query.clone());
            if state.found_for.as_ref() != Some(&key) {
                state.matches = state.query.matches(self.doc.text());
                state.found_for = Some(key);
            }
        }
    }

    /// The bar, when it is open.
    pub(super) fn find_bar(&self) -> Option<Element<'_, Message>> {
        let state = self.find.as_ref()?;
        let message = |input| Message(Input::Find(input));
        let selection = self.doc.selection().range();
        let count = if state.query.text.is_empty() {
            String::new()
        } else if state.matches.is_empty() {
            "No matches".into()
        } else {
            match state.matches.iter().position(|m| *m == selection) {
                Some(i) => format!("{} of {}", i + 1, state.matches.len()),
                None => format!("{} matches", state.matches.len()),
            }
        };
        let case = state.query.case_sensitive;
        let toggle_style = if case {
            button::primary
        } else {
            button::secondary
        };
        let find_row = row![
            text_input("Find", &state.query.text)
                .id(FIND)
                .on_input(move |t| message(FindInput::Query(t)))
                .on_submit(message(FindInput::Step { forward: true }))
                .padding(4)
                .width(Fill),
            text(count).size(13),
            button(text("Aa").size(13))
                .style(toggle_style)
                .on_press(message(FindInput::Case(!case))),
            button(text("Previous").size(13))
                .style(button::secondary)
                .on_press(message(FindInput::Step { forward: false })),
            button(text("Next").size(13))
                .style(button::secondary)
                .on_press(message(FindInput::Step { forward: true })),
            button(text("Close").size(13))
                .style(button::secondary)
                .on_press(message(FindInput::Close)),
        ]
        .spacing(6)
        .align_y(iced::Center);
        let replace_row = state.replacing.then(|| {
            row![
                text_input("Replace", &state.replacement)
                    .id(REPLACE)
                    .on_input(move |t| message(FindInput::Replacement(t)))
                    .on_submit(message(FindInput::Replace))
                    .padding(4)
                    .width(Fill),
                button(text("Replace").size(13))
                    .style(button::secondary)
                    .on_press(message(FindInput::Replace)),
                button(text("Replace all").size(13))
                    .style(button::secondary)
                    .on_press(message(FindInput::ReplaceAll)),
            ]
            .spacing(6)
            .align_y(iced::Center)
        });
        Some(
            column![find_row]
                .push(replace_row)
                .spacing(6)
                .padding([8, 16])
                .into(),
        )
    }
}
