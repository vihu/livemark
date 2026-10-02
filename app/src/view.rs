//! The window (PLAN-002, PLAN-006): the sidebar column and the note's
//! column (`shell.rs`), the vault menu over them, a bar for questions and
//! errors under the note. And the keys and window events the app listens
//! to.
use iced::keyboard;
use iced::widget::{Space, button, column, container, mouse_area, row, stack, text};
use iced::{Element, Length, Subscription, window};

use super::{App, Message};

impl App {
    pub(crate) fn view(&self) -> Element<'_, Message> {
        // The tag manager in the note's place; the editor is made again
        // when it closes (its text, scroll and caret are the `Editor`'s).
        let editor = match &self.manager {
            _ if self.appearance => self.appearance_view(),
            Some(manager) => self.manager_view(manager),
            // A right press on the text: its menu (Cut, Copy, Paste...).
            None => mouse_area(self.editor.view().map(Message::Editor))
                .on_right_press(Message::Context(super::context::ContextMessage::Open(
                    super::context::Target::Text,
                )))
                .into(),
        };
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
        } else if let Some(title) = &self.naming {
            use super::sidebar::{TITLE, VaultMessage};
            Some(
                row![
                    text("New note"),
                    iced::widget::text_input("Title", title)
                        .id(TITLE)
                        .on_input(|title| Message::Vault(VaultMessage::Title(title)))
                        .on_submit(Message::Vault(VaultMessage::Create))
                        .width(Length::Fill),
                    button("Create").on_press(Message::Vault(VaultMessage::Create)),
                    button("Cancel")
                        .style(button::secondary)
                        .on_press(Message::Vault(VaultMessage::CancelNew)),
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
        // The vault menu floats over everything from the sidebar's head (or
        // the note's bar while the sidebar is hidden); a press elsewhere
        // closes it.
        let menu = self.menu.then(|| {
            stack![
                mouse_area(Space::new().width(Length::Fill).height(Length::Fill))
                    .on_press(Message::Menu(false)),
                container(self.vault_menu()).padding(iced::Padding {
                    top: super::shell::BAR - 4.0,
                    left: 8.0,
                    ..iced::Padding::ZERO
                }),
            ]
        });
        // The Undo after a tag edit, low over the note.
        let toast = self.undo_toast().map(|toast| {
            container(toast)
                .width(Length::Fill)
                .height(Length::Fill)
                .align_y(iced::Bottom)
                .padding(iced::Padding {
                    left: 64.0,
                    bottom: 24.0,
                    ..iced::Padding::ZERO
                })
        });
        // The note's column: its bar, the note with the search's drop-down
        // and the Undo note over it, a question under it. Always a column
        // and stacks, so the editor keeps its place in the widget tree (and
        // its focus) when a bar or the menu comes or goes.
        let note = column![
            self.note_bar(),
            self.outside_place(),
            stack![editor].push(self.search_panel()).push(toast)
        ]
        .push(bar.map(|bar| container(bar).padding(12)));
        // Every press says where it landed first, for the menus.
        super::context::press_at(
            stack![row![self.sidebar_column(), note]]
                .push(menu)
                .push(self.resize_layer())
                .push(self.context_layer()),
        )
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
                'q' => Some(Message::CloseRequested),
                '\\' => Some(Message::Sidebar),
                'o' => Some(Message::Open),
                // Ctrl+Shift+F as well, as the sidebar's search had it.
                'p' | 'f' => Some(Message::Search(super::search::SearchMessage::Open)),
                's' => Some(Message::Save {
                    choose: modifiers.shift(),
                }),
                // The whole interface, as browsers do (PLAN-006); the
                // text's own size is in Appearance.
                '=' | '+' => Some(scale(1)),
                '-' => Some(scale(-1)),
                '0' => Some(scale(0)),
                _ => None,
            }
        });
        let close = window::close_requests().map(|_| Message::CloseRequested);
        // The system going light or dark, for System's picks.
        let system = iced::system::theme_changes()
            .map(|mode| Message::Appearance(super::appearance::AppearanceMessage::System(mode)));
        let focus = window::events().filter_map(|(_, event)| match event {
            window::Event::Focused => Some(Message::Focused),
            window::Event::Unfocused => Some(Message::Blurred),
            window::Event::Resized(size) => Some(Message::Resized(size)),
            window::Event::FileDropped(file) => Some(Message::Dropped(file)),
            _ => None,
        });
        // The search's keys: its field leaves Up, Down and Escape alone,
        // and the modifiers say whether Enter lists.
        let search = self.search.is_some().then(|| {
            use super::search::SearchMessage;
            keyboard::listen().filter_map(|event| match event {
                keyboard::Event::KeyPressed {
                    key: keyboard::Key::Named(named),
                    ..
                } => match named {
                    keyboard::key::Named::ArrowUp => Some(Message::Search(SearchMessage::Move(-1))),
                    keyboard::key::Named::ArrowDown => {
                        Some(Message::Search(SearchMessage::Move(1)))
                    }
                    keyboard::key::Named::Escape => Some(Message::Search(SearchMessage::Close)),
                    _ => None,
                },
                keyboard::Event::ModifiersChanged(modifiers) => {
                    Some(Message::Search(SearchMessage::Modifiers(modifiers)))
                }
                _ => None,
            })
        });
        // F2 renames the open note (PLAN-006); Escape takes back a menu or
        // question open under a tag or a note.
        let rename = keyboard::listen().filter_map(|event| match event {
            keyboard::Event::KeyPressed {
                key: keyboard::Key::Named(keyboard::key::Named::F2),
                modifiers,
                ..
            } if modifiers.is_empty() => Some(Message::Note(
                super::note_actions::NoteMessage::StartRename(None),
            )),
            _ => None,
        });
        let asking =
            (self.tag_action.is_some() || self.note_action.is_some() || self.context.is_some())
                .then(|| {
                    keyboard::listen().filter_map(|event| match event {
                        keyboard::Event::KeyPressed {
                            key: keyboard::Key::Named(keyboard::key::Named::Escape),
                            ..
                        } => Some(Message::Escape),
                        _ => None,
                    })
                });
        Subscription::batch(
            [keys, close, focus, rename, system]
                .into_iter()
                .chain(search)
                .chain(asking),
        )
    }
}

/// The interface's scale a step: subscriptions take no captures.
fn scale(step: i8) -> Message {
    Message::Appearance(super::appearance::AppearanceMessage::Scale(step))
}
