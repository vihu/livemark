//! Quick open (PLAN-004 answer 5): Ctrl+P, a field over the notes' titles,
//! fuzzy, most recent first; `#tag` narrows to a tag. Up and Down choose,
//! Enter or a click opens, Escape or a click elsewhere closes. Outside a
//! vault it lists the recent files.
use std::path::PathBuf;

use iced::widget::text::{Ellipsis, Wrapping};
use iced::widget::{Space, button, column, container, mouse_area, opaque, row, text, text_input};
use iced::{Element, Length, Task, Theme};

use super::{App, Message};

/// The field's id, focused when quick open opens.
const FIELD: iced::widget::Id = iced::widget::Id::new("livemark-quick-open");

/// The most notes shown at once.
const SHOWN: usize = 12;

/// The panel's width.
const WIDTH: f32 = 520.0;

#[derive(Debug, Clone)]
pub enum QuickMessage {
    Open,
    Query(String),
    /// Up (-1) or Down (1).
    Move(i32),
    /// Open the chosen note, or this one.
    Choose(Option<usize>),
    Close,
}

/// What quick open shows.
#[derive(Debug, Default)]
pub struct Quick {
    pub query: String,
    pub selected: usize,
    /// Title, the line under it, and the file.
    pub results: Vec<(String, String, PathBuf)>,
}

impl App {
    pub(crate) fn quick_update(&mut self, message: QuickMessage) -> Task<Message> {
        match message {
            QuickMessage::Open => {
                self.menu = false;
                self.quick = Some(Quick::default());
                self.find_quick();
                return iced::widget::operation::focus(FIELD);
            }
            QuickMessage::Query(query) => {
                if let Some(quick) = &mut self.quick {
                    quick.query = query;
                    quick.selected = 0;
                }
                self.find_quick();
            }
            QuickMessage::Move(step) => {
                if let Some(quick) = &mut self.quick {
                    let last = quick.results.len().saturating_sub(1) as i32;
                    quick.selected = (quick.selected as i32 + step).clamp(0, last) as usize;
                }
            }
            QuickMessage::Choose(at) => {
                let Some(quick) = self.quick.take() else {
                    return Task::none();
                };
                let chosen = at.unwrap_or(quick.selected);
                if let Some((_, _, path)) = quick.results.into_iter().nth(chosen) {
                    let task = self.update(Message::Opened(Some(path)));
                    return Task::batch([task, livemark::widget::Editor::focus()]);
                }
            }
            QuickMessage::Close => {
                self.quick = None;
                return livemark::widget::Editor::focus();
            }
        }
        Task::none()
    }

    /// The notes matching the query, best first.
    fn find_quick(&mut self) {
        let Some(quick) = &mut self.quick else {
            return;
        };
        let (tags, words): (Vec<&str>, Vec<&str>) = quick
            .query
            .split_whitespace()
            .partition(|word| word.starts_with('#'));
        let wanted = words.join(" ");
        let mut found: Vec<(i32, usize, (String, String, PathBuf))> = Vec::new();
        match &self.vault {
            Some(vault) => {
                for (rank, note) in vault.notes.iter().enumerate() {
                    let tagged = tags.iter().all(|tag| {
                        let tag = tag.trim_start_matches('#').to_lowercase();
                        note.tags.iter().any(|t| t.starts_with(&tag))
                    });
                    let Some(score) = score(&note.title, &wanted).filter(|_| tagged) else {
                        continue;
                    };
                    let meta = note.created.clone().unwrap_or_default();
                    let meta = note.tags.iter().fold(meta, |line, tag| {
                        format!("{line}  #{tag}").trim_start().to_owned()
                    });
                    found.push((score, rank, (note.title.clone(), meta, note.path.clone())));
                }
            }
            None if tags.is_empty() => {
                for (rank, path) in self.settings.recent.iter().enumerate() {
                    let name = path
                        .file_name()
                        .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
                    if let Some(score) = score(&name, &wanted) {
                        let folder = path
                            .parent()
                            .map_or_else(String::new, |p| p.display().to_string());
                        found.push((score, rank, (name, folder, path.clone())));
                    }
                }
            }
            None => {}
        }
        found.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
        quick.results = found.into_iter().take(SHOWN).map(|(_, _, r)| r).collect();
        quick.selected = quick.selected.min(quick.results.len().saturating_sub(1));
    }

    /// The panel over the text, when quick open is open.
    pub(crate) fn quick_view(&self) -> Option<Element<'_, Message>> {
        let quick = self.quick.as_ref()?;
        let field = text_input("Open a note by its title, #tag to narrow", &quick.query)
            .id(FIELD)
            .on_input(|query| Message::Quick(QuickMessage::Query(query)))
            .on_submit(Message::Quick(QuickMessage::Choose(None)))
            .padding([8, 10]);
        let mut list = column![].spacing(1);
        for (i, (title, meta, _)) in quick.results.iter().enumerate() {
            let on = i == quick.selected;
            let entry = column![
                text(title.as_str())
                    .size(14)
                    .wrapping(Wrapping::None)
                    .ellipsis(Ellipsis::End),
            ]
            .push((!meta.is_empty()).then(|| {
                text(meta.as_str())
                    .size(11)
                    .wrapping(Wrapping::None)
                    .ellipsis(Ellipsis::Start)
                    .style(move |theme: &Theme| text::Style {
                        color: Some(row_text(theme, on).scale_alpha(0.6)),
                    })
            }));
            list = list.push(
                button(entry)
                    .width(Length::Fill)
                    .padding([5, 10])
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
                    .on_press(Message::Quick(QuickMessage::Choose(Some(i)))),
            );
        }
        if quick.results.is_empty() {
            list = list.push(
                container(text("No note matches").size(13).style(text::secondary)).padding([6, 10]),
            );
        }
        let panel = container(column![field, list].spacing(6))
            .width(WIDTH)
            .padding(6)
            .style(container::bordered_box);
        Some(
            iced::widget::stack![
                mouse_area(Space::new().width(Length::Fill).height(Length::Fill))
                    .on_press(Message::Quick(QuickMessage::Close)),
                container(row![
                    Space::new().width(Length::Fill),
                    opaque(panel),
                    Space::new().width(Length::Fill)
                ])
                .padding([24, 12]),
            ]
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

/// How well `query` matches `title`, fuzzily: its characters in order,
/// more for runs, word starts and the title's start; `None` when they are
/// not all there. An empty query matches everything equally.
pub fn score(title: &str, query: &str) -> Option<i32> {
    let title: Vec<char> = title.to_lowercase().chars().collect();
    let mut score = 0;
    let mut at = 0;
    let mut last: Option<usize> = None;
    for wanted in query.to_lowercase().chars().filter(|c| !c.is_whitespace()) {
        let found = (at..title.len()).find(|&i| title[i] == wanted)?;
        score += 1;
        if last.is_some_and(|last| last + 1 == found) {
            score += 4;
        }
        if found == 0 || !title[found - 1].is_alphanumeric() {
            score += 3;
        }
        if found == 0 {
            score += 2;
        }
        score -= (found - at).min(5) as i32 / 2;
        last = Some(found);
        at = found + 1;
    }
    Some(score)
}

#[cfg(test)]
mod tests {
    use super::score;

    #[test]
    fn fuzzy_matches_prefer_runs_and_word_starts() {
        assert!(score("Lisbon hotels", "lh").is_some());
        assert!(score("Lisbon hotels", "hl x").is_none());
        // A run at a word's start beats letters scattered through.
        assert!(score("Lisbon hotels", "hot") > score("Shopping in Oslo, Tallinn", "hot"));
        assert!(score("Standup", "sta") > score("Lisbon status", "sta"));
        assert_eq!(score("Anything", ""), Some(0));
    }
}
