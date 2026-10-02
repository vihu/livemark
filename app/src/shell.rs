//! The window's frame (PLAN-006, toolbar A): the sidebar as a column to the
//! window's top, its head the vault menu (in File's place) with "+" for a
//! new note and the button that hides it (Ctrl+\); the note's column with
//! its own bar: the editor's formatting tools, the search field in the
//! middle, the save state and the mode. Without a vault the sidebar offers
//! to open one and lists the recent files.
use std::path::Path;

use iced::widget::text::{Ellipsis, Wrapping};
use iced::widget::{
    Space, button, column, container, mouse_area, responsive, row, rule, scrollable, text, tooltip,
};
use iced::{Element, Length, Theme, mouse};

use super::icons::{Icon, Tone, icon};
use super::settings::{SIDEBAR_WIDTH, SIDEBAR_WIDTHS};
use super::sidebar::{VaultMessage, choice};
use super::{App, Message};

/// The sidebar's edge dragged.
#[derive(Debug, Clone, Copy)]
pub enum Resize {
    Start,
    /// The pointer at this x, the sidebar's width to be.
    To(f32),
    End,
    /// A double click: the width it starts with.
    Reset,
}

/// The note's state as its bar shows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SaveState {
    /// Edits not written yet.
    Unsaved,
    /// Written, in a file outside the open vault.
    Outside,
    Saved,
    /// A new note with nothing in it.
    New,
}

/// The bars' height, the sidebar's head and the note's.
pub(crate) const BAR: f32 = 48.0;

/// What the bar's sides hold: the editor's tools; the sidebar's buttons
/// while it is hidden; the save state and the mode.
const TOOLS: f32 = 262.0;
const SIDEBAR_BUTTONS: f32 = 205.0;
const STATE_AND_MODE: f32 = 240.0;

/// The command key as the platform names it, for the menus' hints.
pub(crate) const COMMAND: &str = if cfg!(target_os = "macos") {
    "Cmd"
} else {
    "Ctrl"
};

impl App {
    /// The sidebar column; a zero-width space while hidden, so the note
    /// keeps its place in the row.
    pub(crate) fn sidebar_column(&self) -> Element<'_, Message> {
        if !self.settings.sidebar {
            return Space::new().width(0).into();
        }
        let head = row![
            self.panel_button(),
            self.menu_button(),
            Space::new().width(Length::Fill),
            icon_button(Icon::Plus, format!("New note ({COMMAND}+N)"), Message::New),
        ]
        .spacing(2)
        .height(BAR)
        .align_y(iced::Center);
        let body = self.vault_body().unwrap_or_else(|| self.no_vault());
        let side = container(column![head, body])
            .width(self.settings.sidebar_width)
            .height(Length::Fill)
            .padding(iced::Padding {
                top: 0.0,
                right: 8.0,
                bottom: 8.0,
                left: 8.0,
            })
            .style(|theme: &Theme| container::Style {
                background: Some(theme.palette().background.weakest.color.into()),
                ..container::Style::default()
            });
        // Its edge: dragged to resize, a double click back to the start.
        let resizing = self.resizing;
        let line = container(
            Space::new()
                .width(if resizing { 2 } else { 1 })
                .height(Length::Fill),
        )
        .style(move |theme: &Theme| container::Style {
            background: Some(
                if resizing {
                    theme.palette().primary.base.color
                } else {
                    theme.palette().background.strong.color
                }
                .into(),
            ),
            ..container::Style::default()
        });
        let edge = mouse_area(
            row![line, Space::new().width(if resizing { 3 } else { 4 })].height(Length::Fill),
        )
        .interaction(mouse::Interaction::ResizingHorizontally)
        .on_press(Message::Resize(Resize::Start))
        .on_double_click(Message::Resize(Resize::Reset));
        row![side, edge].into()
    }

    /// While the edge is dragged: a layer over the window that follows the
    /// pointer until the button is let go.
    pub(crate) fn resize_layer(&self) -> Option<Element<'_, Message>> {
        self.resizing.then(|| {
            mouse_area(Space::new().width(Length::Fill).height(Length::Fill))
                .interaction(mouse::Interaction::ResizingHorizontally)
                .on_move(|at| Message::Resize(Resize::To(at.x)))
                .on_release(Message::Resize(Resize::End))
                .into()
        })
    }

    pub(crate) fn resize_update(&mut self, resize: Resize) {
        match resize {
            Resize::Start => self.resizing = true,
            Resize::To(x) => {
                self.settings.sidebar_width =
                    x.clamp(*SIDEBAR_WIDTHS.start(), *SIDEBAR_WIDTHS.end());
            }
            Resize::End => {
                self.resizing = false;
                self.remember();
            }
            Resize::Reset => {
                self.resizing = false;
                self.settings.sidebar_width = SIDEBAR_WIDTH;
                self.remember();
            }
        }
    }

    /// The button that hides the sidebar, or brings it back.
    fn panel_button(&self) -> Element<'_, Message> {
        let hint = if self.settings.sidebar {
            "Hide the sidebar"
        } else {
            "Show the sidebar"
        };
        icon_button(
            Icon::Panel,
            format!("{hint} ({COMMAND}+\\)"),
            Message::Sidebar,
        )
    }

    /// The vault's name with the menu under it (File's place).
    fn menu_button(&self) -> Element<'_, Message> {
        let name = self
            .vault
            .as_ref()
            .map_or_else(|| "livemark".to_owned(), |vault| vault.name());
        // Short enough for the bar while the sidebar is hidden.
        let name = if name.chars().count() > 22 {
            name.chars().take(21).chain(['\u{2026}']).collect()
        } else {
            name
        };
        let open = self.menu;
        button(
            row![
                text(name)
                    .size(15)
                    .font(iced::Font {
                        weight: iced::font::Weight::Bold,
                        ..iced::Font::new(livemark::fonts::PROSE)
                    })
                    .wrapping(Wrapping::None)
                    .ellipsis(Ellipsis::End),
                icon(Icon::Chevron, 16.0, Tone::Quiet),
            ]
            .spacing(6)
            .align_y(iced::Center),
        )
        .padding([6, 8])
        .style(move |theme: &Theme, status| ghost(theme, status, open))
        .on_press(Message::Menu(!self.menu))
        .into()
    }

    /// The sidebar without a vault: what one is, the button that opens one,
    /// and the recent files.
    fn no_vault(&self) -> Element<'_, Message> {
        let mut list = column![
            text("No vault open").size(15),
            text("A vault is a folder of notes: tags, search and links across them, kept in your own git repository.")
                .size(12)
                .style(text::secondary),
            button(text("Open vault...").size(13))
                .padding([6, 12])
                .style(button::primary)
                .on_press(Message::Vault(VaultMessage::Open)),
        ]
        .spacing(10)
        .padding([8, 8]);
        if !self.settings.recent.is_empty() {
            list = list.push(Space::new().height(6));
            list = list.push(text("Recent files").size(12).style(text::secondary));
            let mut files = column![].spacing(1);
            for path in &self.settings.recent {
                let on = self.path.as_deref() == Some(path.as_path());
                files = files.push(
                    button(
                        column![
                            text(file_name(path))
                                .size(14)
                                .wrapping(Wrapping::None)
                                .ellipsis(Ellipsis::End),
                            text(folder(path))
                                .size(11)
                                .style(text::secondary)
                                .wrapping(Wrapping::None)
                                .ellipsis(Ellipsis::Start),
                        ]
                        .spacing(1),
                    )
                    .width(Length::Fill)
                    .padding([5, 8])
                    .style(move |theme: &Theme, status| choice(theme, status, on))
                    .on_press(Message::Recent(path.clone())),
                );
            }
            list = list.push(scrollable(files).height(Length::Fill));
        }
        list.into()
    }

    /// The note's bar: the tools (after the sidebar's buttons while it is
    /// hidden), the search field in the middle, the save state and the
    /// mode; a hairline under it.
    pub(crate) fn note_bar(&self) -> Element<'_, Message> {
        let bar = responsive(move |size| -> Element<'_, Message> {
            // Both sides as wide as the wider, the field centred, when that
            // leaves it room; else each side its own width and the field
            // what is left.
            let room = size.width - 24.0 - 24.0;
            let left_need = TOOLS
                + if self.settings.sidebar {
                    0.0
                } else {
                    SIDEBAR_BUTTONS
                };
            let side = left_need.max(STATE_AND_MODE);
            let (left_width, right_width) = if room - 2.0 * side >= 240.0 {
                (side, side)
            } else {
                (left_need, STATE_AND_MODE)
            };
            let width = (room - left_width - right_width).clamp(120.0, 440.0);
            let left = row![]
                .push((!self.settings.sidebar).then(|| {
                    row![self.panel_button(), self.menu_button()]
                        .spacing(2)
                        .align_y(iced::Center)
                }))
                .push(self.editor.toolbar_tools().map(Message::Editor))
                .spacing(10)
                .align_y(iced::Center);
            let right = row![
                self.save_view(),
                self.editor.toolbar_modes().map(Message::Editor)
            ]
            .spacing(12)
            .align_y(iced::Center);
            row![
                container(left).width(left_width),
                container(self.search_field(width))
                    .width(Length::Fill)
                    .center_x(Length::Fill),
                container(right)
                    .width(right_width)
                    .align_x(iced::alignment::Horizontal::Right),
            ]
            .spacing(12)
            .padding([0, 12])
            .height(BAR)
            .align_y(iced::Center)
            .into()
        });
        column![container(bar).height(BAR), rule::horizontal(1)].into()
    }

    /// What the save state says (PLAN-006).
    pub(crate) fn save_state(&self) -> SaveState {
        match &self.path {
            _ if self.unsaved() => SaveState::Unsaved,
            Some(path) if self.outside_vault(path) => SaveState::Outside,
            Some(_) => SaveState::Saved,
            None => SaveState::New,
        }
    }

    /// The save state with a word on it under the pointer; nothing for an
    /// empty new note.
    fn save_view(&self) -> Element<'_, Message> {
        let quiet = |theme: &Theme| text::Style {
            color: Some(theme.palette().background.base.text.scale_alpha(0.62)),
        };
        let path = self.path.as_deref().unwrap_or(Path::new(""));
        let (mark, said, hint): (Element<'_, Message>, &str, String) = match self.save_state() {
            SaveState::Unsaved => {
                let dot = container(Space::new().width(8).height(8)).style(|theme: &Theme| {
                    container::Style {
                        background: Some(theme.palette().primary.base.color.into()),
                        border: iced::Border::default().rounded(4),
                        ..container::Style::default()
                    }
                });
                (dot.into(), "Unsaved", format!("{COMMAND}+S saves"))
            }
            SaveState::Outside => (
                icon(Icon::Folder, 16.0, Tone::Quiet),
                "Not in the vault",
                path.display().to_string(),
            ),
            SaveState::Saved => (
                icon(Icon::Check, 16.0, Tone::Quiet),
                "Saved",
                format!("Saved to {}", file_name(path)),
            ),
            SaveState::New => return Space::new().into(),
        };
        let words = text(said).size(12);
        let words = if said == "Unsaved" {
            words
        } else {
            words.style(quiet)
        };
        tooltip(
            row![mark, words].spacing(6).align_y(iced::Center),
            container(text(hint).size(12)).padding([4, 8]),
            tooltip::Position::Bottom,
        )
        .gap(6)
        .style(container::rounded_box)
        .into()
    }

    /// Whether `path` is a file outside the open vault.
    pub(crate) fn outside_vault(&self, path: &Path) -> bool {
        let Some(vault) = &self.vault else {
            return false;
        };
        if path.starts_with(&vault.root) {
            return false;
        }
        let path = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        !path.starts_with(&vault.root)
    }
}

/// An icon button with no box, shaded under the pointer, its hint under
/// it.
pub(crate) fn icon_button<'a>(glyph: Icon, hint: String, message: Message) -> Element<'a, Message> {
    tooltip(
        button(icon(glyph, 20.0, Tone::Quiet))
            .padding(5)
            .style(|theme: &Theme, status| ghost(theme, status, false))
            .on_press(message),
        container(text(hint).size(12)).padding([4, 8]),
        tooltip::Position::Bottom,
    )
    .gap(6)
    .style(container::rounded_box)
    .into()
}

/// A button with no box: shaded under the pointer, and while `open`.
pub(crate) fn ghost(theme: &Theme, status: button::Status, open: bool) -> button::Style {
    let palette = theme.palette();
    let background = match status {
        _ if open => Some(palette.background.weak.color),
        button::Status::Hovered => Some(palette.background.weak.color),
        button::Status::Pressed => Some(palette.background.strong.color),
        _ => None,
    };
    button::Style {
        background: background.map(iced::Background::Color),
        text_color: palette.background.base.text,
        border: iced::Border::default().rounded(7),
        ..button::Style::default()
    }
}

pub(crate) fn file_name(path: &Path) -> String {
    path.file_name().map_or_else(
        || path.display().to_string(),
        |name| name.to_string_lossy().into_owned(),
    )
}

/// The folder a file is in, the home folder as `~`.
pub(crate) fn folder(path: &Path) -> String {
    let folder = path.parent().unwrap_or(path);
    let home = std::env::var_os("HOME").map(std::path::PathBuf::from);
    match home
        .as_deref()
        .and_then(|home| folder.strip_prefix(home).ok())
    {
        Some(rest) if rest.as_os_str().is_empty() => "~".into(),
        Some(rest) => format!("~/{}", rest.display()),
        None => folder.display().to_string(),
    }
}
