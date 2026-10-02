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
}

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
        }
        Task::none()
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
        Some(
            container(
                column![
                    header,
                    tags,
                    rule::horizontal(1),
                    scrollable(notes).height(Length::Fill),
                ]
                .spacing(8),
            )
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
fn choice(theme: &Theme, status: button::Status, on: bool) -> button::Style {
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
