//! The vault menu (PLAN-006), in File's place at the sidebar's head: new,
//! open, open vault, the recent files, save, save as, Appearance, the
//! interface's zoom, quit.
use iced::widget::text::{Ellipsis, Wrapping};
use iced::widget::{button, column, container, row, rule, text};
use iced::{Element, Length, Theme};

use super::appearance::AppearanceMessage;
use super::icons::{Icon, Tone, icon};
use super::shell::{COMMAND, file_name, folder, ghost};
use super::sidebar::VaultMessage;
use super::{App, Message};

/// Width of the open menu.
const MENU_WIDTH: f32 = 300.0;

impl App {
    /// The vault menu: new, open, open vault, the recent files, save, save
    /// as, Appearance, the interface's zoom, quit.
    pub(crate) fn vault_menu(&self) -> Element<'_, Message> {
        let item = |glyph: Icon, label: &'static str, key: &str, message: Message| {
            button(
                row![
                    icon(glyph, 16.0, Tone::Quiet),
                    text(label).size(14).width(Length::Fill),
                    text(if key.is_empty() {
                        String::new()
                    } else {
                        format!("{COMMAND}+{key}")
                    })
                    .size(12)
                    .style(text::secondary),
                ]
                .spacing(10)
                .align_y(iced::Center),
            )
            .width(Length::Fill)
            .padding([7, 10])
            .style(|theme: &Theme, status| ghost(theme, status, false))
            .on_press(message)
        };
        let mut items = column![
            item(Icon::Plus, "New note", "N", Message::New),
            item(Icon::Folder, "Open file...", "O", Message::Open),
            item(
                Icon::Vault,
                "Open vault...",
                "",
                Message::Vault(VaultMessage::Open)
            ),
        ];
        if !self.settings.recent.is_empty() {
            items = items.push(rule::horizontal(1));
            items = items.push(
                container(text("Recent files").size(12).style(text::secondary)).padding([4, 10]),
            );
            for path in self.settings.recent.iter().take(6) {
                let note = column![
                    text(file_name(path))
                        .size(14)
                        .wrapping(Wrapping::None)
                        .ellipsis(Ellipsis::End),
                    text(folder(path))
                        .size(11)
                        .style(text::secondary)
                        .wrapping(Wrapping::None)
                        .ellipsis(Ellipsis::Start),
                ];
                items = items.push(
                    button(row![icon(Icon::Clock, 16.0, Tone::Quiet), note].spacing(10))
                        .width(Length::Fill)
                        .padding([5, 10])
                        .style(|theme: &Theme, status| ghost(theme, status, false))
                        .on_press(Message::Recent(path.clone())),
                );
            }
        }
        items = items
            .push(rule::horizontal(1))
            .push(item(
                Icon::Save,
                "Save",
                "S",
                Message::Save { choose: false },
            ))
            .push(item(
                Icon::Save,
                "Save as...",
                "Shift+S",
                Message::Save { choose: true },
            ))
            .push(rule::horizontal(1));
        let step = |glyph: Icon, message: Message| {
            button(icon(glyph, 12.0, Tone::Quiet))
                .padding([5, 7])
                .style(button::secondary)
                .on_press(message)
        };
        let scale = |step| Message::Appearance(AppearanceMessage::Scale(step));
        items = items
            .push(item(
                Icon::Palette,
                "Appearance...",
                "",
                Message::Appearance(AppearanceMessage::Open),
            ))
            .push(
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
                .padding([4, 10])
                .align_y(iced::Center),
            )
            .push(rule::horizontal(1))
            .push(item(Icon::Quit, "Quit", "Q", Message::CloseRequested));
        container(items.spacing(2))
            .width(MENU_WIDTH)
            .padding(4)
            .style(container::bordered_box)
            .into()
    }
}
