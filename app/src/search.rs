//! Search (PLAN-005): one field in the toolbar's centre, Ctrl+P. Results
//! drop down in three groups: notes whose title matches (fuzzily, the most
//! recent first among equals), lines in other notes with every word, and
//! tags. `#tag` words narrow to notes with a tag starting so. Up and Down
//! choose; Enter opens a note (a line with its match selected) or shows a
//! tag's notes in the sidebar; Escape closes; Ctrl+Enter lists every
//! matching note in the sidebar, with its lines, in place of the notes.
//! Outside a vault the field finds the recent files by name. No index: the
//! notes are in memory, a lowercase copy of each kept for this.
use std::ops::Range;
use std::path::PathBuf;

use iced::{Task, keyboard};

use super::matching::title_match;
use super::vault::{Note, Vault};
use super::{App, Message};

/// The field's id, focused by Ctrl+P.
pub(crate) const FIELD: iced::widget::Id = iced::widget::Id::new("livemark-search");

/// The most notes listed in the sidebar, and lines a note.
pub(crate) const NOTES: usize = 200;
const LINES: usize = 5;

/// What the drop-down shows of each group; notes when nothing is typed.
const TITLES: usize = 5;
const IN_TEXT: usize = 5;
const TAGS: usize = 4;
const RECENT: usize = 8;

/// What is shown around a match, in characters each side.
const AROUND: usize = 40;

#[derive(Debug, Clone)]
pub enum SearchMessage {
    /// Ctrl+P: the field focused, the results shown.
    Open,
    Query(String),
    /// Up (-1) or Down (1).
    Move(i32),
    /// Enter (with Ctrl held, the list in the sidebar), or this result
    /// clicked.
    Choose(Option<usize>),
    /// The modifiers held, while the results show.
    Modifiers(keyboard::Modifiers),
    /// Open this note with this range selected.
    Go(PathBuf, Range<usize>),
    Close,
    /// The sidebar's list of matches gone, the notes back.
    Clear,
}

/// One result in the drop-down.
#[derive(Debug, Clone, PartialEq)]
pub enum Found {
    /// A note by its title: the title, the line under it, its file, and
    /// the title's stretches that matched.
    Note(String, String, PathBuf, Vec<Range<usize>>),
    /// A line in a note: its title, the text around the match, the line's
    /// number, the file and the match's range in it.
    Line {
        title: String,
        snippet: String,
        number: usize,
        path: PathBuf,
        range: Range<usize>,
    },
    /// A tag and how many notes have it.
    Tag(String, usize),
}

/// The drop-down, while it shows.
#[derive(Debug, Default)]
pub struct Search {
    pub query: String,
    pub selected: usize,
    pub found: Vec<Found>,
    /// How many notes have every word and tag (Ctrl+Enter lists them).
    pub count: usize,
    /// Ctrl (Cmd on macOS) held: Enter lists.
    command: bool,
}

/// A note that matched: its title, file and lines (number, the text
/// around the match, the match's range in the note).
#[derive(Debug, Clone)]
pub struct Hit {
    pub title: String,
    pub path: PathBuf,
    pub lines: Vec<(usize, String, Range<usize>)>,
}

/// Every match, listed in the sidebar (Ctrl+Enter) until cleared.
#[derive(Debug)]
pub struct Listing {
    pub query: String,
    pub hits: Vec<Hit>,
    pub count: usize,
    /// A new one for every listing, for the cached view.
    pub generation: u64,
}

/// The query's words and `#tag` prefixes, lowercase, `#` off.
pub(crate) fn split(query: &str) -> (Vec<String>, Vec<String>) {
    let (tags, words): (Vec<String>, Vec<String>) = query
        .split_whitespace()
        .map(str::to_lowercase)
        .partition(|word| word.starts_with('#'));
    let tags = tags
        .into_iter()
        .map(|tag| tag.trim_start_matches('#').to_owned())
        .collect();
    (words, tags)
}

/// Whether `note` has a tag starting with each of `tags`.
pub(crate) fn tagged(note: &Note, tags: &[String]) -> bool {
    tags.iter()
        .all(|tag| note.tags.iter().any(|t| t.starts_with(tag.as_str())))
}

/// The drop-down's results for `query`, in its groups, and how many notes
/// match.
pub fn find(vault: &Vault, query: &str) -> (Vec<Found>, usize) {
    let (words, tags) = split(query);
    let wanted = words.join(" ");
    let mut titles: Vec<(i32, usize, &Note, Vec<Range<usize>>)> = vault
        .notes
        .iter()
        .enumerate()
        .filter(|(_, note)| tagged(note, &tags))
        .filter_map(|(rank, note)| {
            let (score, marks) = title_match(&note.title, &wanted)?;
            Some((score, rank, note, marks))
        })
        .collect();
    titles.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    titles.truncate(if words.is_empty() { RECENT } else { TITLES });
    let mut found: Vec<Found> = titles
        .iter()
        .map(|(_, _, note, marks)| {
            let meta = note.created.clone().unwrap_or_default();
            let meta = note.tags.iter().fold(meta, |line, tag| {
                format!("{line}  #{tag}").trim_start().to_owned()
            });
            Found::Note(note.title.clone(), meta, note.path.clone(), marks.clone())
        })
        .collect();
    let (hits, count) = search(vault, query);
    let lines = hits
        .into_iter()
        .filter(|hit| !titles.iter().any(|(_, _, note, _)| note.path == hit.path))
        .filter_map(|hit| {
            let (number, snippet, range) = hit.lines.into_iter().next()?;
            Some(Found::Line {
                title: hit.title,
                snippet,
                number,
                path: hit.path,
                range,
            })
        })
        .take(IN_TEXT);
    found.extend(lines);
    let prefixes: Vec<&String> = words.iter().chain(&tags).collect();
    if !prefixes.is_empty() {
        let names = vault
            .tags()
            .into_iter()
            .filter(|(tag, _)| prefixes.iter().any(|p| tag.starts_with(p.as_str())))
            .take(TAGS)
            .map(|(tag, count)| Found::Tag(tag, count));
        found.extend(names);
    }
    (found, count)
}

/// The recent files whose names match `query`, outside a vault.
fn find_recent(recent: &[PathBuf], query: &str) -> Vec<Found> {
    let mut found: Vec<(i32, usize, Found)> = Vec::new();
    for (rank, path) in recent.iter().enumerate() {
        let name = path
            .file_name()
            .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
        if let Some((score, marks)) = title_match(&name, query) {
            let folder = path
                .parent()
                .map_or_else(String::new, |p| p.display().to_string());
            found.push((score, rank, Found::Note(name, folder, path.clone(), marks)));
        }
    }
    found.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    found.into_iter().take(RECENT).map(|(_, _, f)| f).collect()
}

/// The notes of `vault` with every word and tag in `query`, most recent
/// first, and how many there are.
pub fn search(vault: &Vault, query: &str) -> (Vec<Hit>, usize) {
    let (words, tags) = split(query);
    if words.is_empty() && tags.is_empty() {
        return (Vec::new(), 0);
    }
    let mut hits = Vec::new();
    let mut count = 0;
    for note in &vault.notes {
        if !tagged(note, &tags) || !words.iter().all(|word| note.lower.contains(word.as_str())) {
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
                self.menu = false;
                let query = self.search.take().map(|s| s.query).unwrap_or_default();
                self.search = Some(Search {
                    query,
                    ..Search::default()
                });
                self.find();
                return iced::widget::operation::focus(FIELD);
            }
            SearchMessage::Query(query) => {
                let search = self.search.get_or_insert_with(Search::default);
                search.query = query;
                search.selected = 0;
                self.find();
            }
            SearchMessage::Move(step) => {
                if let Some(search) = &mut self.search {
                    let last = search.found.len().saturating_sub(1) as i32;
                    search.selected = (search.selected as i32 + step).clamp(0, last) as usize;
                }
            }
            SearchMessage::Modifiers(modifiers) => {
                if let Some(search) = &mut self.search {
                    search.command = modifiers.command();
                }
            }
            SearchMessage::Choose(None) if self.search.as_ref().is_some_and(|s| s.command) => {
                return self.list();
            }
            SearchMessage::Choose(at) => {
                let Some(search) = self.search.take() else {
                    return Task::none();
                };
                let chosen = search.found.into_iter().nth(at.unwrap_or(search.selected));
                let task = match chosen {
                    Some(Found::Note(_, _, path, _)) => self.update(Message::Opened(Some(path))),
                    Some(Found::Line { path, range, .. }) => {
                        return self.search_update(SearchMessage::Go(path, range));
                    }
                    Some(Found::Tag(tag, _)) => {
                        self.listing = None;
                        self.shown = super::sidebar::Shown::Tag(tag);
                        Task::none()
                    }
                    None => Task::none(),
                };
                return Task::batch([task, livemark::widget::Editor::focus()]);
            }
            SearchMessage::Go(path, range) => {
                self.search = None;
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
            SearchMessage::Clear => self.listing = None,
        }
        Task::none()
    }

    /// The drop-down's results for its query.
    fn find(&mut self) {
        let Some(search) = &mut self.search else {
            return;
        };
        (search.found, search.count) = match &self.vault {
            Some(vault) => find(vault, &search.query),
            None => (find_recent(&self.settings.recent, &search.query), 0),
        };
        search.selected = search.selected.min(search.found.len().saturating_sub(1));
    }

    /// Ctrl+Enter: every match listed in the sidebar.
    fn list(&mut self) -> Task<Message> {
        if self.vault.is_none() {
            return Task::none();
        }
        let (Some(search), Some(vault)) = (self.search.take(), &self.vault) else {
            return Task::none();
        };
        let (hits, count) = self::search(vault, &search.query);
        let generation = self.listing.as_ref().map_or(0, |l| l.generation + 1);
        self.listing = Some(Listing {
            query: search.query,
            hits,
            count,
            generation,
        });
        livemark::widget::Editor::focus()
    }
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
