//! Search across the vault (PLAN-004 answer 5): Ctrl+Shift+F, in the
//! sidebar. Every word must be in a note (any case); `#tag` words narrow
//! to notes with a tag starting so. Each note with its matching lines; a
//! click opens it with the match selected. No index: the notes are in
//! memory, a lowercase copy of each kept for this.
use std::ops::Range;
use std::path::PathBuf;

use iced::widget::text::{Ellipsis, Wrapping};
use iced::widget::{Space, button, column, container, lazy, row, scrollable, text, text_input};
use iced::{Element, Length, Task, Theme};

use super::vault::Vault;
use super::{App, Message};

/// The field's id, focused when search opens.
const FIELD: iced::widget::Id = iced::widget::Id::new("livemark-search");

/// The most notes listed, and lines a note.
const NOTES: usize = 200;
const LINES: usize = 5;

/// What is shown around a match, in characters each side.
const AROUND: usize = 40;

#[derive(Debug, Clone)]
pub enum SearchMessage {
    Open,
    Query(String),
    /// Open this note with this range selected.
    Go(PathBuf, Range<usize>),
    /// Enter: the first match.
    First,
    Close,
}

/// A note that matched: its title, file and lines (number, the text
/// around the match, the match's range in the note).
#[derive(Debug, Clone)]
pub struct Hit {
    pub title: String,
    pub path: PathBuf,
    pub lines: Vec<(usize, String, Range<usize>)>,
}

#[derive(Debug, Default)]
pub struct Search {
    pub query: String,
    pub hits: Vec<Hit>,
    /// How many notes matched, past the ones listed too.
    pub count: usize,
    /// Changes with every query, for the cached view.
    generation: u64,
}

/// The notes of `vault` matching `query`, most recent first.
pub fn search(vault: &Vault, query: &str) -> (Vec<Hit>, usize) {
    let (tags, words): (Vec<String>, Vec<String>) = query
        .split_whitespace()
        .map(str::to_lowercase)
        .partition(|word| word.starts_with('#'));
    if words.is_empty() && tags.is_empty() {
        return (Vec::new(), 0);
    }
    let mut hits = Vec::new();
    let mut count = 0;
    for note in &vault.notes {
        let tagged = tags.iter().all(|tag| {
            let tag = tag.trim_start_matches('#');
            note.tags.iter().any(|t| t.starts_with(tag))
        });
        if !tagged || !words.iter().all(|word| note.lower.contains(word.as_str())) {
            continue;
        }
        count += 1;
        if hits.len() >= NOTES {
            continue;
        }
        hits.push(Hit {
            title: note.title.clone(),
            path: note.path.clone(),
            lines: lines(&note.text, &words),
        });
    }
    (hits, count)
}

/// The lines of `text` with one of `words` in them: their number, the text
/// around the first match and the match's range.
fn lines(text: &str, words: &[String]) -> Vec<(usize, String, Range<usize>)> {
    let mut found = Vec::new();
    let mut start = 0;
    for (number, line) in text.split_inclusive('\n').enumerate() {
        let at = start;
        start += line.len();
        let line = line.trim_end_matches(['\n', '\r']);
        let lower = line.to_lowercase();
        let Some((offset, word)) = words
            .iter()
            .filter_map(|w| lower.find(w.as_str()).map(|i| (i, w)))
            .min_by_key(|(i, _)| *i)
        else {
            continue;
        };
        // Lowercasing kept the bytes where they were: the match's own
        // range; else the line's.
        let range = if lower.len() == line.len() {
            at + offset..at + offset + word.len()
        } else {
            at..at + line.len()
        };
        found.push((number + 1, around(line, range.start - at), range));
        if found.len() == LINES {
            break;
        }
    }
    found
}

/// Up to `AROUND` characters each side of byte `at` in `line`, trimmed.
fn around(line: &str, at: usize) -> String {
    let at = (0..=at.min(line.len()))
        .rev()
        .find(|&i| line.is_char_boundary(i))
        .unwrap_or(0);
    let before: String = line[..at]
        .chars()
        .rev()
        .take(AROUND)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    let after: String = line[at..].chars().take(AROUND + 20).collect();
    let cut_left = before.len() < at;
    let cut_right = after.len() < line.len() - at;
    format!(
        "{}{}{}{}",
        if cut_left { "\u{2026}" } else { "" },
        before.trim_start(),
        after.trim_end(),
        if cut_right { "\u{2026}" } else { "" }
    )
}

impl App {
    pub(crate) fn search_update(&mut self, message: SearchMessage) -> Task<Message> {
        match message {
            SearchMessage::Open => {
                if self.vault.is_none() {
                    return Task::none();
                }
                self.menu = false;
                self.quick = None;
                if self.search.is_none() {
                    self.search = Some(Search::default());
                }
                return iced::widget::operation::focus(FIELD);
            }
            SearchMessage::Query(query) => {
                if let (Some(search), Some(vault)) = (&mut self.search, &self.vault) {
                    (search.hits, search.count) = self::search(vault, &query);
                    search.query = query;
                    search.generation += 1;
                }
            }
            SearchMessage::First => {
                let first = self.search.as_ref().and_then(|search| {
                    let hit = search.hits.first()?;
                    let range = hit.lines.first().map_or(0..0, |line| line.2.clone());
                    Some((hit.path.clone(), range))
                });
                if let Some((path, range)) = first {
                    return self.search_update(SearchMessage::Go(path, range));
                }
            }
            SearchMessage::Go(path, range) => {
                let task = self.update(Message::Opened(Some(path.clone())));
                if self.path.as_ref() == Some(&path) {
                    self.editor.select(range.start, range.end);
                }
                return Task::batch([task, livemark::widget::Editor::focus()]);
            }
            SearchMessage::Close => {
                self.search = None;
                return livemark::widget::Editor::focus();
            }
        }
        Task::none()
    }

    /// The sidebar while searching: the field and the matches.
    pub(crate) fn search_view<'a>(&'a self, search: &'a Search) -> Element<'a, Message> {
        let field = row![
            text_input("Search notes, #tag to narrow", &search.query)
                .id(FIELD)
                .on_input(|query| Message::Search(SearchMessage::Query(query)))
                .on_submit(Message::Search(SearchMessage::First))
                .padding([6, 8]),
            button(text("\u{d7}").size(16))
                .padding([2, 8])
                .style(button::text)
                .on_press(Message::Search(SearchMessage::Close)),
        ]
        .spacing(4)
        .align_y(iced::Center);
        let summary = match (search.query.trim().is_empty(), search.count) {
            (true, _) => String::new(),
            (false, 0) => "No note matches".into(),
            (false, 1) => "1 note".into(),
            (false, n) if n > NOTES => format!("{n} notes, the {NOTES} most recent shown"),
            (false, n) => format!("{n} notes"),
        };
        let current = self.path.clone();
        let hits = lazy((search.generation, current), move |(_, current)| {
            results(&search.hits, current.as_deref())
        });
        column![
            field,
            text(summary).size(11).style(text::secondary),
            scrollable(hits).height(Length::Fill),
        ]
        .spacing(8)
        .into()
    }
}

/// The matches: each note's title, then its lines, every one a link to
/// the match.
fn results(hits: &[Hit], current: Option<&std::path::Path>) -> Element<'static, Message> {
    let mut list = column![].spacing(2);
    for hit in hits {
        let first = hit.lines.first().map_or(0..0, |line| line.2.clone());
        let on = current == Some(hit.path.as_path());
        list = list.push(
            button(
                text(hit.title.clone())
                    .size(14)
                    .wrapping(Wrapping::None)
                    .ellipsis(Ellipsis::End),
            )
            .width(Length::Fill)
            .padding([4, 8])
            .style(move |theme: &Theme, status| super::sidebar::choice(theme, status, on))
            .on_press(Message::Search(SearchMessage::Go(hit.path.clone(), first))),
        );
        for (number, snippet, range) in &hit.lines {
            list = list.push(
                button(
                    row![
                        text(number.to_string())
                            .size(11)
                            .width(28)
                            .style(text::secondary),
                        text(snippet.clone())
                            .size(12)
                            .wrapping(Wrapping::None)
                            .ellipsis(Ellipsis::End),
                    ]
                    .spacing(4),
                )
                .width(Length::Fill)
                .padding([2, 8])
                .style(|theme: &Theme, status| super::sidebar::choice(theme, status, false))
                .on_press(Message::Search(SearchMessage::Go(
                    hit.path.clone(),
                    range.clone(),
                ))),
            );
        }
        list = list.push(Space::new().height(4));
    }
    container(list).into()
}

#[cfg(test)]
mod tests {
    use super::{around, lines};

    #[test]
    fn matching_lines_carry_their_range_and_some_context() {
        let text = "first line\nThe Hotel near Alfama\r\nnothing\nhotel again\n";
        let found = lines(text, &["hotel".to_owned()]);
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].0, 2);
        assert_eq!(&text[found[0].2.clone()], "Hotel");
        assert_eq!(found[1].1, "hotel again");
        let long = format!("{}needle{}", "a ".repeat(60), "b ".repeat(60));
        let shown = around(&long, long.find("needle").unwrap());
        assert!(
            shown.starts_with('\u{2026}')
                && shown.contains("needle")
                && shown.ends_with('\u{2026}')
        );
        // A line whose lowercase is longer: the whole line is the range.
        let text = "\u{130}stanbul hotel\n";
        assert_eq!(lines(text, &["hotel".to_owned()])[0].2, 0..text.len() - 1);
    }
}
