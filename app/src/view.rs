//! The window (PLAN-002): the toolbar, with the File menu before the
//! editor's own buttons; the editor; a bar for questions and errors under
//! it. And the keys and window events the app listens to.
use std::path::Path;

use iced::keyboard;
use iced::widget::text::{Ellipsis, Wrapping};
use iced::widget::{Space, button, column, container, mouse_area, opaque, row, rule, stack, text};
use iced::{Element, Length, Subscription, Theme, window};

use super::{App, Message, settings};

/// Width of the open File menu.
const MENU_WIDTH: f32 = 300.0;

/// The command key as the platform names it, for the menu's hints.
const COMMAND: &str = if cfg!(target_os = "macos") {
    "Cmd"
} else {
    "Ctrl"
};

impl App {
    pub(crate) fn view(&self) -> Element<'_, Message> {
        let editor = self.editor.view().map(Message::Editor);
        let bar: Option<Element<'_, Message>> = if self.changed {
            Some(
                row![
                    text("The file changed on disk.").width(Length::Fill),
                    button("Load it")
                        .style(button::danger)
                        .on_press(Message::Reload(true)),
                    button("Keep mine")
                        .style(button::secondary)
                        .on_press(Message::Reload(false)),
                ]
                .spacing(8)
                .align_y(iced::Center)
                .into(),
            )
        } else if self.pending.is_some() {
            Some(
                row![
                    text("Unsaved changes.").width(Length::Fill),
                    button("Save").on_press(Message::Unsaved(Some(true))),
                    button("Discard")
                        .style(button::danger)
                        .on_press(Message::Unsaved(Some(false))),
                    button("Cancel")
                        .style(button::secondary)
                        .on_press(Message::Unsaved(None)),
                ]
                .spacing(8)
                .align_y(iced::Center)
                .into(),
            )
        } else {
            self.error
                .as_ref()
                .map(|error| text(error).style(text::danger).into())
        };
        // As the editor's toolbar buttons: an outline, shaded under the
        // pointer, filled while open.
        let open = self.menu;
        let file = button(
            container(text("File").size(13))
                .height(20)
                .align_y(iced::Center),
        )
        .padding([5, 10])
        .style(move |theme: &Theme, status| {
            let palette = theme.palette();
            let (background, text_color) = match status {
                _ if open => (Some(palette.primary.base.color), palette.primary.base.text),
                button::Status::Hovered => (
                    Some(palette.background.weak.color),
                    palette.background.base.text,
                ),
                button::Status::Pressed => (
                    Some(palette.background.strong.color),
                    palette.background.base.text,
                ),
                _ => (None, palette.background.base.text),
            };
            button::Style {
                background: background.map(iced::Background::Color),
                text_color,
                border: iced::Border::default().rounded(5),
                ..button::Style::default()
            }
        })
        .on_press(Message::Menu(!self.menu));
        let file = container(file)
            .padding(2)
            .style(|theme: &Theme| container::Style {
                border: iced::Border {
                    color: theme.palette().background.strong.color,
                    width: 1.0,
                    radius: 7.0.into(),
                },
                ..container::Style::default()
            });
        // The toolbar always on top (PLAN-002).
        let toolbar = container(
            row![
                file,
                Space::new().width(8),
                self.editor.toolbar().map(Message::Editor)
            ]
            .align_y(iced::Center),
        )
        .padding([6, 12]);
        // The open menu floats over the text; a press anywhere else closes
        // it. Always a column and a stack, so the editor keeps its place in
        // the widget tree (and its focus) when a bar or the menu comes or
        // goes.
        let menu = self.menu.then(|| {
            stack![
                mouse_area(Space::new().width(Length::Fill).height(Length::Fill))
                    .on_press(Message::Menu(false)),
                container(opaque(self.menu())).padding([0, 12]),
            ]
        });
        // The vault's sidebar left of the text; a zero-width space without
        // one, so the editor keeps its place in the row.
        let sidebar = self
            .sidebar()
            .unwrap_or_else(|| Space::new().width(0).into());
        column![toolbar, stack![row![sidebar, editor]].push(menu)]
            .push(bar.map(|bar| container(bar).padding(12)))
            .into()
    }

    /// The File menu: new, open and save; the recent notes; the theme.
    fn menu(&self) -> Element<'_, Message> {
        let item = |label: &'static str, key: &str, message: Message| {
            button(
                row![
                    text(label).size(14).width(Length::Fill),
                    text(if key.is_empty() {
                        String::new()
                    } else {
                        format!("{COMMAND}+{key}")
                    })
                    .size(12)
                    .style(text::secondary),
                ]
                .spacing(12)
                .align_y(iced::Center),
            )
            .width(Length::Fill)
            .padding([6, 10])
            .style(button::text)
            .on_press(message)
        };
        let mut items = column![
            item("New", "N", Message::New),
            item("Open...", "O", Message::Open),
            item("Save", "S", Message::Save { choose: false }),
            item("Save as...", "Shift+S", Message::Save { choose: true }),
            item(
                "Open vault...",
                "",
                Message::Vault(super::sidebar::VaultMessage::Open)
            ),
        ];
        if !self.settings.recent.is_empty() {
            items = items.push(rule::horizontal(1));
            items = items
                .push(container(text("Recent").size(12).style(text::secondary)).padding([4, 10]));
            for path in &self.settings.recent {
                let name = path.file_name().map_or_else(
                    || path.display().to_string(),
                    |name| name.to_string_lossy().into_owned(),
                );
                let note = column![
                    text(name)
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
                    button(note)
                        .width(Length::Fill)
                        .padding([4, 10])
                        .style(button::text)
                        .on_press(Message::Recent(path.clone())),
                );
            }
        }
        // As shown: a `--dark` or `--light` flag wins over the settings.
        let shown = match &self.theme {
            None => settings::Theme::System,
            Some(Theme::Dark) => settings::Theme::Dark,
            Some(_) => settings::Theme::Light,
        };
        let theme = |label: &'static str, theme: settings::Theme| {
            button(text(label).size(13))
                .padding([4, 10])
                .style(if theme == shown {
                    button::primary
                } else {
                    button::text
                })
                .on_press(Message::Theme(theme))
        };
        items = items.push(rule::horizontal(1));
        items = items.push(
            row![
                text("Theme").size(14).width(Length::Fill),
                theme("System", settings::Theme::System),
                theme("Light", settings::Theme::Light),
                theme("Dark", settings::Theme::Dark),
            ]
            .spacing(2)
            .padding([4, 10])
            .align_y(iced::Center),
        );
        container(items.spacing(2))
            .width(MENU_WIDTH)
            .padding(4)
            .style(container::bordered_box)
            .into()
    }

    pub(crate) fn subscription(&self) -> Subscription<Message> {
        // By the letter on the key in the layout, as the editor reads its
        // own shortcuts.
        let keys = keyboard::listen().filter_map(|event| {
            let keyboard::Event::KeyPressed {
                key,
                physical_key,
                modifiers,
                ..
            } = event
            else {
                return None;
            };
            if !modifiers.command() || modifiers.alt() {
                return None;
            }
            match key.to_latin(physical_key)? {
                'n' => Some(Message::New),
                'o' => Some(Message::Open),
                's' => Some(Message::Save {
                    choose: modifiers.shift(),
                }),
                '=' | '+' => Some(Message::Zoom(1)),
                '-' => Some(Message::Zoom(-1)),
                '0' => Some(Message::Zoom(0)),
                _ => None,
            }
        });
        let close = window::close_requests().map(|_| Message::CloseRequested);
        let focus = window::events().filter_map(|(_, event)| match event {
            window::Event::Focused => Some(Message::Focused),
            window::Event::Resized(size) => Some(Message::Resized(size)),
            window::Event::FileDropped(file) => Some(Message::Dropped(file)),
            _ => None,
        });
        Subscription::batch([keys, close, focus])
    }
}

/// The folder a recent note is in, the home folder as `~`.
fn folder(path: &Path) -> String {
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
