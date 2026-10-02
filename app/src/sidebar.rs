//! The vault's sidebar (PLAN-004): its tags with counts, a click filtering
//! the notes by one, and its notes, most recently changed first, a click
//! opening one.
use std::path::PathBuf;

use iced::widget::text::{Ellipsis, Wrapping};
use iced::widget::{Space, button, column, container, lazy, row, rule, scrollable, text};
use iced::{Element, Length, Task, Theme};

use super::vault::Vault;
use super::{App, Message, file};

/// The sidebar's width.
const WIDTH: f32 = 260.0;

/// The most notes listed; Ctrl+P and search reach the rest.
const SHOWN: usize = 500;

/// What the vault's parts of the window ask for.
#[derive(Debug, Clone)]
pub enum VaultMessage {
    /// File > Open vault: pick a folder.
    Open,
    /// The folder picked (`None`: the dialog was cancelled).
    Picked(Option<PathBuf>),
    /// Show only the notes with this tag; the same again, or `None`, shows
    /// all.
    Tag(Option<String>),
    /// Ctrl+N in a vault: ask for the new note's title.
    NewNote,
    /// The title typed so far.
    Title(String),
    /// Make the note with that title, and open it.
    Create,
    /// No new note after all.
    CancelNew,
}

/// The new note's title field, focused when it opens.
pub const TITLE: iced::widget::Id = iced::widget::Id::new("livemark-new-note");

impl App {
    pub(crate) fn vault_update(&mut self, message: VaultMessage) -> Task<Message> {
        match message {
            VaultMessage::Open => {
                self.menu = false;
                return Task::perform(file::pick_folder(), |folder| {
                    Message::Vault(VaultMessage::Picked(folder))
                });
            }
            VaultMessage::Picked(Some(root)) => match Vault::open(&root) {
                Ok(vault) => {
                    self.settings.vault = Some(vault.root.clone());
                    self.remember();
                    self.vault = Some(vault);
                    self.tag = None;
                }
                Err(error) => self.error = Some(error.to_string()),
            },
            VaultMessage::Picked(None) => {}
            VaultMessage::Tag(tag) => {
                self.tag = if tag == self.tag { None } else { tag };
            }
            VaultMessage::NewNote => {
                self.menu = false;
                self.naming = Some(String::new());
                return iced::widget::operation::focus(TITLE);
            }
            VaultMessage::Title(title) => self.naming = Some(title),
            VaultMessage::CancelNew => self.naming = None,
            VaultMessage::Create => return self.create_note(),
        }
        Task::none()
    }

    /// The note named in the title field, made in the vault's folder and
    /// opened with the caret after its front matter.
    fn create_note(&mut self) -> Task<Message> {
        let (Some(vault), Some(title)) = (&self.vault, self.naming.take()) else {
            return Task::none();
        };
        let title = title.trim();
        let title = if title.is_empty() { "Untitled" } else { title };
        let header = crate::note::Header {
            title,
            tags: &[],
            created: &crate::note::today(),
            by: None,
        };
        match crate::note::create(&vault.root, &header, "") {
            Ok(path) => {
                self.refresh_vault();
                let task = self.update(Message::Opened(Some(path.clone())));
                if self.path.as_ref() == Some(&path) {
                    let end = self.editor.text().len();
                    self.editor.select(end, end);
                }
                task
            }
            Err(error) => {
                self.error = Some(format!("{}: {error}", vault.root.display()));
                Task::none()
            }
        }
    }

    /// A `#tag` clicked in a note: the sidebar shows the notes with it.
    pub(crate) fn show_tag(&mut self, tag: &str) {
        if self.vault.is_some() {
            self.search = None;
            self.tag = Some(tag.to_lowercase());
        }
    }

    /// Reads again what changed in the vault on disk.
    pub(crate) fn refresh_vault(&mut self) {
        if let Some(vault) = &mut self.vault {
            vault.refresh();
        }
    }

    /// The sidebar, when a vault is open.
    pub(crate) fn sidebar(&self) -> Option<Element<'_, Message>> {
        let vault = self.vault.as_ref()?;
        let chip = |label: String, tag: Option<String>, on: bool| {
            button(text(label).size(12))
                .padding([3, 8])
                .style(move |theme: &Theme, status| choice(theme, status, on))
                .on_press(Message::Vault(VaultMessage::Tag(tag)))
                .into()
        };
        let mut chips: Vec<Element<'_, Message>> = vec![chip(
            format!("All {}", vault.notes.len()),
            None,
            self.tag.is_none(),
        )];
        for (tag, count) in vault.tags() {
            let on = self.tag.as_deref() == Some(tag.as_str());
            chips.push(chip(format!("#{tag} {count}"), Some(tag), on));
        }
        // A few rows of tags; past about a dozen, they scroll.
        let many = chips.len() > 12;
        let tags = scrollable(row(chips).spacing(4).wrap().vertical_spacing(4)).height(if many {
            Length::Fixed(132.0)
        } else {
            Length::Shrink
        });
        let current = self.path.clone();
        let filter = self.tag.clone();
        let notes = lazy(
            (vault.generation, filter, current),
            |(_, filter, current)| notes(vault, filter.as_deref(), current.as_deref()),
        );
        let header = text(vault.name()).size(13).style(text::secondary);
        // While searching, the search in place of the tags and notes.
        let body: Element<'_, Message> = match &self.search {
            Some(search) => self.search_view(search),
            None => column![
                tags,
                rule::horizontal(1),
                scrollable(notes).height(Length::Fill)
            ]
            .push(
                self.linked_from()
                    .map(|linked| column![rule::horizontal(1), linked].spacing(8)),
            )
            .spacing(8)
            .into(),
        };
        // How much is not committed yet: the user's git keeps the vault.
        let status = vault.uncommitted().map(|count| {
            let line = match count {
                0 => "All notes committed".to_owned(),
                1 => "1 note changed since the last commit".to_owned(),
                n => format!("{n} notes changed since the last commit"),
            };
            text(line).size(11).style(text::secondary)
        });
        Some(
            container(column![header, body].push(status).spacing(8))
                .width(WIDTH)
                .height(Length::Fill)
                .padding([8, 8])
                .style(|theme: &Theme| container::Style {
                    background: Some(theme.palette().background.weakest.color.into()),
                    ..container::Style::default()
                })
                .into(),
        )
    }
}

/// The note list: titles and dates, the open note marked.
fn notes(
    vault: &Vault,
    tag: Option<&str>,
    current: Option<&std::path::Path>,
) -> Element<'static, Message> {
    let shown = vault
        .notes
        .iter()
        .filter(|note| tag.is_none_or(|tag| note.tags.iter().any(|t| t == tag)));
    let mut list = column![].spacing(1);
    let mut count = 0;
    for note in shown {
        count += 1;
        if count > SHOWN {
            continue;
        }
        let on = current == Some(note.path.as_path());
        let meta = match (&note.created, note.tags.is_empty()) {
            (Some(date), _) => format!("{date}  {}", tags_line(&note.tags)),
            (None, false) => tags_line(&note.tags),
            (None, true) => String::new(),
        };
        let row = column![
            text(note.title.clone())
                .size(14)
                .wrapping(Wrapping::None)
                .ellipsis(Ellipsis::End),
        ]
        .push((!meta.is_empty()).then(|| {
            text(meta)
                .size(11)
                .style(move |theme: &Theme| {
                    let palette = theme.palette();
                    let color = if on {
                        palette.primary.weak.text
                    } else {
                        palette.background.base.text
                    };
                    text::Style {
                        color: Some(color.scale_alpha(0.6)),
                    }
                })
                .wrapping(Wrapping::None)
                .ellipsis(Ellipsis::End)
        }));
        list = list.push(
            button(row)
                .width(Length::Fill)
                .padding([5, 8])
                .style(move |theme: &Theme, status| choice(theme, status, on))
                .on_press(Message::Opened(Some(note.path.clone()))),
        );
    }
    if count > SHOWN {
        list = list.push(
            container(
                text(format!(
                    "{} more: Ctrl+P or search finds them",
                    count - SHOWN
                ))
                .size(11)
                .style(text::secondary),
            )
            .padding([6, 8]),
        );
    }
    if count == 0 {
        list = list.push(Space::new().height(4));
    }
    list.into()
}

fn tags_line(tags: &[String]) -> String {
    tags.iter()
        .map(|tag| format!("#{tag}"))
        .collect::<Vec<_>>()
        .join(" ")
}

/// A sidebar button: plain, shaded under the pointer, filled when chosen.
pub(crate) fn choice(theme: &Theme, status: button::Status, on: bool) -> button::Style {
    let palette = theme.palette();
    let background = match status {
        _ if on => Some(palette.primary.weak.color),
        button::Status::Hovered => Some(palette.background.weak.color),
        button::Status::Pressed => Some(palette.background.strong.color),
        _ => None,
    };
    button::Style {
        background: background.map(iced::Background::Color),
        text_color: if on {
            palette.primary.weak.text
        } else {
            palette.background.base.text
        },
        border: iced::Border::default().rounded(5),
        ..button::Style::default()
    }
}
