//! Right-click menus (PLAN-006), one kind everywhere: on a note in the
//! sidebar, on a tag (or its "···" button), and on the note's text. The
//! menu floats where the pointer pressed, over everything; a press outside
//! it closes it (a right press opening another where it lands), as does
//! Escape or picking an item. Where the pointer pressed comes from
//! `PressAt`, a wrapper round the whole window that says so before any
//! widget under it hears of the press.
use std::path::PathBuf;

use iced::advanced::layout;
use iced::advanced::mouse;
use iced::advanced::overlay;
use iced::advanced::renderer;
use iced::advanced::widget::{Operation, Tree};
use iced::advanced::{Layout, Shell, Widget};
use iced::widget::{Space, button, column, container, mouse_area, opaque, row, rule, stack, text};
use iced::{Element, Event, Length, Point, Rectangle, Size, Task, Theme, Vector};

use super::icons::{Icon, Tone, icon};
use super::note_actions::NoteMessage;
use super::shell::COMMAND;
use super::sidebar::{Shown, VaultMessage, choice};
use super::tag_actions::TagMessage;
use super::{App, Message};

/// The menu's width.
const WIDTH: f32 = 240.0;

/// What a menu is for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    Note(PathBuf),
    Tag(String),
    /// The note's text.
    Text,
}

/// A menu open at a point of the window.
#[derive(Debug, Clone, PartialEq)]
pub struct Menu {
    pub at: Point,
    pub target: Target,
}

#[derive(Debug, Clone)]
pub enum ContextMessage {
    /// A menu for this, where the pointer last pressed.
    Open(Target),
    Close,
    /// An item picked: the menu closed, then this done.
    Pick(Box<Message>),
    Cut,
    Copy,
    Paste,
    SelectAll,
}

impl App {
    pub(crate) fn context_update(&mut self, message: ContextMessage) -> Task<Message> {
        match message {
            ContextMessage::Open(target) => {
                self.tag_action = None;
                self.note_action = None;
                self.context = Some(Menu {
                    at: self.pointer,
                    target,
                });
            }
            ContextMessage::Close => self.context = None,
            ContextMessage::Pick(message) => {
                self.context = None;
                return self.update(*message);
            }
            ContextMessage::Copy | ContextMessage::Cut => {
                self.context = None;
                let range = self.editor.selection().range();
                if range.is_empty() {
                    return Task::none();
                }
                let picked = self.editor.text()[range].to_owned();
                if matches!(message, ContextMessage::Cut) {
                    self.editor.insert_text("");
                }
                return iced::clipboard::write(picked).discard();
            }
            ContextMessage::Paste => {
                self.context = None;
                return iced::clipboard::read_text()
                    .map(|text| Message::Paste(text.ok().map(|text| text.to_string())));
            }
            ContextMessage::SelectAll => {
                self.context = None;
                let end = self.editor.text().len();
                self.editor.select(0, end);
            }
        }
        Task::none()
    }

    /// The open menu over the window, with the press-catcher under it.
    pub(crate) fn context_layer(&self) -> Option<Element<'_, Message>> {
        let menu = self.context.as_ref()?;
        let items = self.context_items(&menu.target);
        let rows = items.iter().filter(|i| i.is_some()).count() as f32;
        let rules = items.iter().filter(|i| i.is_none()).count() as f32;
        let height = rows * 29.0 + rules * 9.0 + 10.0;
        // Kept in the window: left of the pointer near the right edge,
        // above it near the bottom.
        let (width, tall) = self.settings.window.unwrap_or((1200.0, 800.0));
        let scale = self.settings.scale.max(0.1);
        let (width, tall) = (width / scale, tall / scale);
        let x = if menu.at.x + WIDTH + 8.0 > width {
            (menu.at.x - WIDTH).max(8.0)
        } else {
            menu.at.x
        };
        let y = if menu.at.y + height + 8.0 > tall {
            (menu.at.y - height).max(8.0)
        } else {
            menu.at.y
        };
        let mut list = column![].spacing(1);
        for item in items {
            list = list.push(match item {
                Some(item) => item,
                None => container(rule::horizontal(1)).padding([4, 6]).into(),
            });
        }
        let panel = container(list)
            .width(WIDTH)
            .padding(4)
            .style(container::bordered_box);
        Some(
            stack![
                mouse_area(Space::new().width(Length::Fill).height(Length::Fill))
                    .on_press(Message::Context(ContextMessage::Close)),
                container(opaque(panel)).padding(iced::Padding {
                    top: y,
                    left: x,
                    ..iced::Padding::ZERO
                }),
            ]
            .into(),
        )
    }

    /// A menu's items; `None` a rule between groups.
    fn context_items(&self, target: &Target) -> Vec<Option<Element<'_, Message>>> {
        let pick = |message: Message| Message::Context(ContextMessage::Pick(Box::new(message)));
        match target {
            Target::Note(path) => {
                let open = self.path.as_ref() == Some(path);
                let note = |message| Some(pick(Message::Note(message)));
                vec![
                    Some(item(
                        Some(Icon::Doc),
                        "Open",
                        "",
                        false,
                        Some(pick(Message::Opened(Some(path.clone())))),
                    )),
                    Some(item(
                        Some(Icon::Pen),
                        "Rename...",
                        if open { "F2" } else { "" },
                        false,
                        note(NoteMessage::StartRename(Some(path.clone()))),
                    )),
                    Some(item(
                        Some(Icon::Copy),
                        "Duplicate",
                        "",
                        false,
                        note(NoteMessage::Duplicate(path.clone())),
                    )),
                    Some(item(
                        Some(Icon::Link),
                        "Copy link",
                        "",
                        false,
                        note(NoteMessage::CopyLink(path.clone())),
                    )),
                    Some(item(
                        Some(Icon::Folder),
                        "Show in folder",
                        "",
                        false,
                        note(NoteMessage::ShowInFolder(path.clone())),
                    )),
                    None,
                    Some(item(
                        Some(Icon::Bin),
                        "Delete...",
                        "",
                        true,
                        note(NoteMessage::StartDelete(path.clone())),
                    )),
                ]
            }
            Target::Tag(tag) => {
                let tags = |message| Some(pick(Message::Tag(message)));
                vec![
                    Some(item(
                        Some(Icon::Doc),
                        "Show its notes",
                        "",
                        false,
                        Some(pick(Message::Vault(VaultMessage::Show(Shown::Tag(
                            tag.clone(),
                        ))))),
                    )),
                    Some(item(
                        Some(Icon::Pen),
                        "Rename or merge...",
                        "",
                        false,
                        tags(TagMessage::StartRename(tag.clone())),
                    )),
                    Some(item(
                        Some(Icon::Palette),
                        "Open in the tag manager",
                        "",
                        false,
                        tags(TagMessage::Manage(tag.clone())),
                    )),
                    None,
                    Some(item(
                        Some(Icon::Bin),
                        "Delete from every note...",
                        "",
                        true,
                        tags(TagMessage::StartDelete(tag.clone())),
                    )),
                ]
            }
            Target::Text => {
                let selected = !self.editor.selection().range().is_empty();
                let when = |on: bool, message| on.then(|| Message::Context(message));
                let key = |letter: &str| format!("{COMMAND}+{letter}");
                vec![
                    Some(item(
                        None,
                        "Cut",
                        &key("X"),
                        false,
                        when(selected, ContextMessage::Cut),
                    )),
                    Some(item(
                        None,
                        "Copy",
                        &key("C"),
                        false,
                        when(selected, ContextMessage::Copy),
                    )),
                    Some(item(
                        None,
                        "Paste",
                        &key("V"),
                        false,
                        when(true, ContextMessage::Paste),
                    )),
                    None,
                    Some(item(
                        None,
                        "Select all",
                        &key("A"),
                        false,
                        when(true, ContextMessage::SelectAll),
                    )),
                ]
            }
        }
    }
}

/// A menu item: its icon, label and key, the whole row the button; with
/// no message it shows, faint, and does nothing.
fn item<'a>(
    glyph: Option<Icon>,
    label: &'static str,
    key: &str,
    danger: bool,
    message: Option<Message>,
) -> Element<'a, Message> {
    let tone = if danger { Tone::Danger } else { Tone::Quiet };
    button(
        row![]
            .push(glyph.map(|glyph| icon(glyph, 16.0, tone)))
            .push(text(label).size(13).width(Length::Fill))
            .push(text(key.to_owned()).size(11).style(text::secondary))
            .spacing(10)
            .align_y(iced::Center),
    )
    .width(Length::Fill)
    .padding([7, 10])
    .style(move |theme: &Theme, status| {
        let mut style = choice(theme, status, false);
        if danger {
            style.text_color = theme.palette().danger.base.color;
        }
        if status == button::Status::Disabled {
            style.text_color = style.text_color.scale_alpha(0.4);
        }
        style
    })
    .on_press_maybe(message)
    .into()
}

/// Wraps the window: every press says where it landed (`Message::Pressed`)
/// before anything under it hears of it.
pub struct PressAt<'a> {
    content: Element<'a, Message>,
}

pub fn press_at<'a>(content: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
    Element::new(PressAt {
        content: content.into(),
    })
}

impl Widget<Message, Theme, iced::Renderer> for PressAt<'_> {
    fn diff(&mut self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_mut(&mut self.content));
    }

    fn size(&self) -> Size<Length> {
        self.content.as_widget().size()
    }

    fn layout(&mut self, tree: &mut Tree, renderer: &iced::Renderer, limits: &layout::Limits) {
        self.content
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, limits);
        tree.size = tree.children[0].size;
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout,
        viewport: &Rectangle,
        renderer: &iced::Renderer,
        operation: &mut dyn Operation,
    ) {
        self.content.as_widget_mut().operate(
            &mut tree.children[0],
            layout,
            viewport,
            renderer,
            operation,
        );
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout,
        cursor: mouse::Cursor,
        renderer: &iced::Renderer,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        if let Event::Mouse(mouse::Event::ButtonPressed(button)) = event
            && let Some(at) = cursor.position()
        {
            shell.publish(Message::Pressed {
                at,
                right: *button == mouse::Button::Right,
            });
        }
        self.content.as_widget_mut().update(
            &mut tree.children[0],
            event,
            layout,
            cursor,
            renderer,
            shell,
            viewport,
        );
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &iced::Renderer,
    ) -> mouse::Interaction {
        self.content.as_widget().mouse_interaction(
            &tree.children[0],
            layout,
            cursor,
            viewport,
            renderer,
        )
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut iced::Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        self.content.as_widget().draw(
            &tree.children[0],
            renderer,
            theme,
            style,
            layout,
            cursor,
            viewport,
        );
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout,
        renderer: &iced::Renderer,
        viewport: &Rectangle,
        translation: Vector,
        window: Size,
    ) -> Vec<overlay::Element<'b, Message, Theme, iced::Renderer>> {
        self.content.as_widget_mut().overlay(
            &mut tree.children[0],
            layout,
            renderer,
            viewport,
            translation,
            window,
        )
    }
}
