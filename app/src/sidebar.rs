//! The vault's sidebar (PLAN-004, PLAN-005): all notes, the untagged ones
//! and the tags, a flat list, most used first (the top eight, then all),
//! a click showing only their notes; the notes, most recently changed
//! first, a click opening one.
use std::path::PathBuf;

use iced::widget::text::{Ellipsis, Wrapping};
use iced::widget::{Space, button, column, container, lazy, row, rule, scrollable, text};
use iced::{Element, Length, Task, Theme};

use super::vault::Vault;
use super::{App, Message, file};

/// The most notes listed; the search (Ctrl+P) reaches the rest.
const SHOWN: usize = 500;

/// The tags listed before "All N tags".
const TOP: usize = 8;

/// A tag row's height, for the list's scrolling height.
const ROW: f32 = 30.0;

/// Which notes the sidebar lists.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub enum Shown {
    #[default]
    All,
    /// The notes with this tag.
    Tag(String),
    /// The notes with no tag.
    Untagged,
}

impl Shown {
    fn keeps(&self, note: &super::vault::Note) -> bool {
        match self {
            Shown::All => true,
            Shown::Tag(tag) => note.tags.iter().any(|t| t == tag),
            Shown::Untagged => note.tags.is_empty(),
        }
    }

    fn heading(&self) -> String {
        match self {
            Shown::All => "Notes".into(),
            Shown::Tag(tag) => format!("Notes tagged #{tag}"),
            Shown::Untagged => "Notes with no tag".into(),
        }
    }
}

/// What the vault's parts of the window ask for.
#[derive(Debug, Clone)]
pub enum VaultMessage {
    /// File > Open vault: pick a folder.
    Open,
    /// The folder picked (`None`: the dialog was cancelled).
    Picked(Option<PathBuf>),
    /// Show these notes; the same tag again shows all.
    Show(Shown),
    /// "All N tags", or back to the top eight.
    AllTags,
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
                    self.shown = Shown::All;
                    self.refresh_footer();
                }
                Err(error) => self.error = Some(error.to_string()),
            },
            VaultMessage::Picked(None) => {}
            VaultMessage::Show(shown) => {
                self.listing = None;
                let again = matches!(shown, Shown::Tag(_)) && shown == self.shown;
                self.shown = if again { Shown::All } else { shown };
            }
            VaultMessage::AllTags => self.all_tags = !self.all_tags,
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
            self.listing = None;
            self.shown = Shown::Tag(tag.to_lowercase());
        }
    }

    /// Reads again what changed in the vault on disk.
    pub(crate) fn refresh_vault(&mut self) {
        if let Some(vault) = &mut self.vault {
            vault.refresh();
        }
        self.refresh_footer();
    }

    /// The sidebar's tags, notes and commit status, when a vault is open
    /// (`shell.rs` puts its head over them).
    pub(crate) fn vault_body(&self) -> Option<Element<'_, Message>> {
        let vault = self.vault.as_ref()?;
        let item = |mark: &'static str, name: String, count: usize, shown: Shown| {
            let on = self.shown == shown;
            button(
                row![
                    text(mark).size(14).width(16).style(text::secondary),
                    text(name)
                        .size(14)
                        .width(Length::Fill)
                        .wrapping(Wrapping::None)
                        .ellipsis(Ellipsis::End),
                    text(count.to_string()).size(12).style(text::secondary),
                ]
                .spacing(4)
                .align_y(iced::Center),
            )
            .width(Length::Fill)
            .padding([5, 8])
            .style(move |theme: &Theme, status| choice(theme, status, on))
            .on_press(Message::Vault(VaultMessage::Show(shown)))
        };
        let untagged = vault.notes.iter().filter(|n| n.tags.is_empty()).count();
        let all = vault.tags();
        let listed = if self.all_tags {
            all.len()
        } else {
            all.len().min(TOP)
        };
        let rows = column(
            all[..listed]
                .iter()
                .map(|(tag, count)| self.tag_row(tag, *count)),
        )
        .spacing(2);
        // All of them past the top eight scroll in eight rows' room.
        let rows = scrollable(rows).height(if listed > TOP {
            Length::Fixed(TOP as f32 * ROW)
        } else {
            Length::Shrink
        });
        let fold = (all.len() > TOP).then(|| {
            let label = if self.all_tags {
                "Fewer tags".to_owned()
            } else {
                format!("All {} tags", all.len())
            };
            button(text(label).size(12).style(text::secondary))
                .padding([4, 8])
                .style(button::text)
                .on_press(Message::Vault(VaultMessage::AllTags))
        });
        let open = self.manager.is_some();
        let manage = button(
            row![
                text("Manage tags").size(13).width(Length::Fill),
                text(if self.suggestion().is_some() {
                    "1 suggestion"
                } else {
                    ""
                })
                .size(11)
                .style(text::secondary),
            ]
            .align_y(iced::Center),
        )
        .width(Length::Fill)
        .padding([5, 8])
        .style(move |theme: &Theme, status| choice(theme, status, open))
        .on_press(Message::Manager(if open {
            super::manager::ManagerMessage::Close
        } else {
            super::manager::ManagerMessage::Open(None)
        }));
        let tags = column![
            item(" ", "All notes".into(), vault.notes.len(), Shown::All),
            item(" ", "Untagged".into(), untagged, Shown::Untagged),
            text("Tags").size(12).style(text::secondary),
            rows,
        ]
        .push(fold)
        .push(manage)
        .spacing(2);
        // The notes shown, or every match the search listed (Ctrl+Enter).
        let (said, clear, notes): (String, Option<Message>, Element<'_, Message>) =
            match &self.listing {
                Some(listing) => (
                    super::search_view::listing_heading(listing),
                    Some(Message::Search(super::search::SearchMessage::Clear)),
                    self.listing_view(listing),
                ),
                None => {
                    let current = self.path.clone();
                    (
                        self.shown.heading(),
                        (self.shown != Shown::All)
                            .then_some(Message::Vault(VaultMessage::Show(Shown::All))),
                        lazy(
                            (vault.generation, self.shown.clone(), current),
                            |(_, shown, current)| notes(vault, shown, current.as_deref()),
                        )
                        .into(),
                    )
                }
            };
        let heading = row![
            text(said)
                .size(12)
                .style(text::secondary)
                .width(Length::Fill),
        ]
        .push(clear.map(|clear| {
            button(text("Clear").size(12))
                .padding([0, 4])
                .style(|theme: &Theme, _| button::Style {
                    text_color: theme.palette().primary.base.color,
                    ..button::Style::default()
                })
                .on_press(clear)
        }))
        .align_y(iced::Center);
        let body = column![
            tags,
            rule::horizontal(1),
            heading,
            scrollable(notes).height(Length::Fill)
        ]
        .spacing(8);
        // How much is not committed yet: the user's git keeps the vault.
        let status = vault.uncommitted().map(|count| {
            let line = match count {
                0 => "All notes committed".to_owned(),
                1 => "1 note changed since the last commit".to_owned(),
                n => format!("{n} notes changed since the last commit"),
            };
            text(line).size(11).style(text::secondary)
        });
        Some(column![body].push(status).spacing(8).into())
    }
}

/// The note list: titles and dates, the open note marked.
fn notes(
    vault: &Vault,
    shown: &Shown,
    current: Option<&std::path::Path>,
) -> Element<'static, Message> {
    let shown = vault.notes.iter().filter(|note| shown.keeps(note));
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
                    "{} more: the search (Ctrl+P) finds them",
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
