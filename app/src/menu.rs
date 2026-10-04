//! The vault menu (PLAN-006), in File's place at the sidebar's head: new,
//! open, Open recent and Switch vault (each a list flying out beside its
//! row, on hover or a click), save, save as, Appearance, the interface's
//! zoom, quit; the version under them.
use std::path::Path;

use iced::widget::text::{Ellipsis, Wrapping};
use iced::widget::{Space, button, column, container, mouse_area, opaque, row, rule, text};
use iced::{Element, Length, Theme};

use super::appearance::AppearanceMessage;
use super::icons::{Icon, Tone, icon};
use super::shell::{COMMAND, file_name, folder, ghost};
use super::sidebar::VaultMessage;
use super::{App, Message};

/// The menu's width, and a list's beside it.
const WIDTH: f32 = 260.0;
const LIST_WIDTH: f32 = 320.0;

/// Every row's height, the space between rows, a rule's row and the
/// menu's padding: where a list flies out lines up with its row.
const ROW: f32 = 34.0;
const GAP: f32 = 2.0;
const RULE: f32 = 9.0;
const PAD: f32 = 4.0;

/// How many recent files the list shows.
const RECENT: usize = 10;

/// A list beside the menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Submenu {
    Recent,
    Vaults,
}

impl App {
    /// The menu, and the list open beside it.
    pub(crate) fn vault_menu(&self) -> Element<'_, Message> {
        let leave = Message::Submenu(None);
        let item = |glyph: Icon, label: &'static str, key: &str, message: Message| {
            let key = if key.is_empty() {
                String::new()
            } else {
                format!("{COMMAND}+{key}")
            };
            mouse_area(
                button(
                    row![
                        icon(glyph, 16.0, Tone::Quiet),
                        text(label).size(14).width(Length::Fill),
                        text(key).size(12).style(text::secondary),
                    ]
                    .spacing(10)
                    .align_y(iced::Center),
                )
                .width(Length::Fill)
                .height(ROW)
                .padding([0, 10])
                .style(|theme: &Theme, status| ghost(theme, status, false))
                .on_press(message),
            )
            // Over another row, a list open beside closes.
            .on_enter(leave.clone())
        };
        let opens = |glyph: Icon, label: &'static str, submenu: Submenu| {
            let open = self.submenu == Some(submenu);
            mouse_area(
                button(
                    row![
                        icon(glyph, 16.0, Tone::Quiet),
                        text(label).size(14).width(Length::Fill),
                        icon(Icon::ChevronRight, 14.0, Tone::Quiet),
                    ]
                    .spacing(10)
                    .align_y(iced::Center),
                )
                .width(Length::Fill)
                .height(ROW)
                .padding([0, 10])
                .style(move |theme: &Theme, status| ghost(theme, status, open))
                .on_press(Message::Submenu(Some(submenu))),
            )
            .on_enter(Message::Submenu(Some(submenu)))
        };
        let line = || container(rule::horizontal(1)).height(RULE).center_y(RULE);
        let step = |glyph: Icon, message: Message| {
            button(icon(glyph, 12.0, Tone::Quiet))
                .padding([5, 7])
                .style(button::secondary)
                .on_press(message)
        };
        let scale = |step| Message::Appearance(AppearanceMessage::Scale(step));
        let zoom = mouse_area(
            row![
                text("Zoom").size(14).width(Length::Fill),
                step(Icon::Minus, scale(-1)),
                text(format!("{:.0}%", self.settings.scale * 100.0))
                    .size(13)
                    .width(48)
                    .center(),
                step(Icon::Plus, scale(1)),
            ]
            .spacing(6)
            .padding([0, 10])
            .height(ROW)
            .align_y(iced::Center),
        )
        .on_enter(leave.clone());
        let items = column![
            item(Icon::Plus, "New note", "N", Message::New),
            item(Icon::Folder, "Open file...", "O", Message::Open),
            opens(Icon::Clock, "Open recent", Submenu::Recent),
            line(),
            opens(Icon::Vault, "Switch vault", Submenu::Vaults),
            line(),
            item(Icon::Save, "Save", "S", Message::Save { choose: false }),
            item(
                Icon::Save,
                "Save as...",
                "Shift+S",
                Message::Save { choose: true }
            ),
            line(),
            item(
                Icon::Palette,
                "Appearance...",
                "",
                Message::Appearance(AppearanceMessage::Open)
            ),
            zoom,
            line(),
            item(Icon::Quit, "Quit", "Q", Message::CloseRequested),
            line(),
            // As roughdraft's menu footer: which build is running, where a
            // Flatpak or a macOS bundle shows no version anywhere else.
            container(
                text(concat!("livemark ", env!("CARGO_PKG_VERSION")))
                    .size(12)
                    .style(text::secondary)
            )
            .padding([6, 10]),
        ]
        .spacing(GAP);
        let menu = container(items)
            .width(WIDTH)
            .padding(PAD)
            .style(container::bordered_box);
        // Beside its row: Open recent is the third, Switch vault the fourth
        // after a rule.
        let list = self.submenu.map(|submenu| {
            let (rows, rules) = match submenu {
                Submenu::Recent => (2.0, 0.0),
                Submenu::Vaults => (3.0, 1.0),
            };
            let top = rows * (ROW + GAP) + rules * (RULE + GAP);
            let panel = match submenu {
                Submenu::Recent => self.recent_list(),
                Submenu::Vaults => self.vault_list(),
            };
            container(opaque(
                container(panel)
                    .width(LIST_WIDTH)
                    .padding(PAD)
                    .style(container::bordered_box),
            ))
            .padding(iced::Padding {
                top,
                ..iced::Padding::ZERO
            })
        });
        row![opaque(menu)].push(list).spacing(2).into()
    }

    /// The recent files: a note's title in the open vault, else the
    /// file's name; under it where it is.
    fn recent_list(&self) -> Element<'_, Message> {
        let mut list = column![].spacing(GAP);
        for path in self.settings.recent.iter().take(RECENT) {
            let (title, place) = self.recent_label(path);
            list = list.push(two_lines(
                Icon::Doc,
                title,
                place,
                false,
                Some(Message::Recent(path.clone())),
            ));
        }
        if self.settings.recent.is_empty() {
            list = list.push(
                container(text("No recent files").size(13).style(text::secondary)).padding([8, 10]),
            );
        }
        list.into()
    }

    /// The vaults opened, the open one marked; Open vault, Close vault.
    fn vault_list(&self) -> Element<'_, Message> {
        let open = self.vault.as_ref().map(|vault| vault.root.clone());
        let mut list = column![].spacing(GAP);
        for root in &self.settings.vaults {
            let here = open.as_ref() == Some(root);
            list = list.push(two_lines(
                Icon::Vault,
                file_name(root),
                folder(root),
                here,
                (!here).then(|| Message::Vault(VaultMessage::Picked(Some(root.clone())))),
            ));
        }
        if !self.settings.vaults.is_empty() {
            list = list.push(container(rule::horizontal(1)).height(RULE).center_y(RULE));
        }
        let plain = |glyph: Icon, label: &'static str, message: Message| {
            button(
                row![
                    icon(glyph, 16.0, Tone::Quiet),
                    text(label).size(14).width(Length::Fill),
                ]
                .spacing(10)
                .align_y(iced::Center),
            )
            .width(Length::Fill)
            .height(ROW)
            .padding([0, 10])
            .style(|theme: &Theme, status| ghost(theme, status, false))
            .on_press(message)
        };
        list = list.push(plain(
            Icon::Folder,
            "Open vault...",
            Message::Vault(VaultMessage::Open),
        ));
        if open.is_some() {
            list = list.push(plain(
                Icon::Close,
                "Close vault",
                Message::Vault(VaultMessage::Close),
            ));
        }
        list.into()
    }

    /// A recent file as the list shows it: its title and where it is.
    pub(crate) fn recent_label(&self, path: &Path) -> (String, String) {
        let stem = || {
            path.file_stem()
                .map_or_else(|| file_name(path), |s| s.to_string_lossy().into_owned())
        };
        let within = |root: &Path, name: String| {
            let inner = path
                .strip_prefix(root)
                .ok()
                .and_then(Path::parent)
                .filter(|p| !p.as_os_str().is_empty());
            match inner {
                Some(inner) => format!("{name} / {}", inner.display()),
                None => name,
            }
        };
        if let Some(vault) = &self.vault
            && path.starts_with(&vault.root)
        {
            let title = vault
                .notes
                .iter()
                .find(|note| note.path == path)
                .map_or_else(stem, |note| note.title.clone());
            return (title, within(&vault.root, vault.name()));
        }
        match self
            .settings
            .vaults
            .iter()
            .find(|root| path.starts_with(root))
        {
            Some(root) => (stem(), within(root, file_name(root))),
            None => (file_name(path), folder(path)),
        }
    }
}

/// A list row: an icon, a line and a quieter line under it, a check when
/// `on`; with no message it does nothing.
fn two_lines<'a>(
    glyph: Icon,
    title: String,
    under: String,
    on: bool,
    message: Option<Message>,
) -> Element<'a, Message> {
    button(
        row![
            icon(glyph, 16.0, Tone::Quiet),
            column![
                text(title)
                    .size(14)
                    .wrapping(Wrapping::None)
                    .ellipsis(Ellipsis::End),
                text(under)
                    .size(11)
                    .style(text::secondary)
                    .wrapping(Wrapping::None)
                    .ellipsis(Ellipsis::Start),
            ]
            .width(Length::Fill),
        ]
        .push(on.then(|| icon(Icon::Check, 16.0, Tone::Quiet)))
        .push(Space::new().width(2))
        .spacing(10)
        .align_y(iced::Center),
    )
    .width(Length::Fill)
    .padding([5, 10])
    .style(move |theme: &Theme, status| ghost(theme, status, on))
    .on_press_maybe(message)
    .into()
}
