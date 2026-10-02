//! The toolbar (PLAN-003, PLAN-006): formatting and line prefixes as icons
//! in two groups with a hairline between them, no outlines, and the mode
//! as three icons in one soft switch at the right edge, named and
//! explained in their tooltips. Each button runs its key's command and
//! gives the keyboard back to the text. A host shows it where it likes,
//! with its own items around it, or its two halves with its own between.
use iced::widget::{Space, button, container, row, text, tooltip};
use iced::{Background, Border, Element, Length, Theme};

use super::icons::{Glyph, icon};
use super::{Editor, Input, Key, Message, Mode};
use crate::edit::blocks::Block;
use crate::edit::format::Format;

impl Editor {
    /// The toolbar: bold, italic, code, link; heading, bullet, task, quote;
    /// and the mode: live preview, markdown, side by side. It fills the
    /// width it is given. Opt-in: nothing shows it unless the host puts it
    /// in its view.
    pub fn toolbar(&self) -> Element<'_, Message> {
        row![
            self.toolbar_tools(),
            Space::new().width(Length::Fill),
            self.toolbar_modes()
        ]
        .width(Length::Fill)
        .align_y(iced::Center)
        .into()
    }

    /// The toolbar's left half: the formatting and line prefix groups, a
    /// hairline between them.
    pub fn toolbar_tools(&self) -> Element<'_, Message> {
        let tool = |glyph: Glyph, hint: &'static str, key: Key| {
            tip(
                button(icon(glyph, false))
                    .padding(5)
                    .style(|theme, status| style(theme, status, false))
                    .on_press(Message(Input::Tool(key))),
                hint,
            )
        };
        let format = row![
            tool(Glyph::Bold, "Bold (Ctrl+B)", Key::Format(Format::Bold)),
            tool(
                Glyph::Italic,
                "Italic (Ctrl+I)",
                Key::Format(Format::Italic),
            ),
            tool(
                Glyph::Code,
                "Inline code (Ctrl+E)",
                Key::Format(Format::Code),
            ),
            tool(Glyph::Link, "Link (Ctrl+K)", Key::Link),
        ]
        .spacing(2);
        let blocks = row![
            tool(
                Glyph::Heading,
                "Heading: #, ##, ###, then none",
                Key::Block(Block::Heading),
            ),
            tool(Glyph::List, "Bullet list", Key::Block(Block::Bullet)),
            tool(Glyph::Task, "Task list", Key::Block(Block::Task)),
            tool(Glyph::Quote, "Quote", Key::Block(Block::Quote)),
        ]
        .spacing(2);
        let hairline =
            container(Space::new().width(1).height(18)).style(|theme: &Theme| container::Style {
                background: Some(theme.palette().background.strong.color.into()),
                ..container::Style::default()
            });
        row![format, hairline, blocks]
            .spacing(6)
            .align_y(iced::Center)
            .into()
    }

    /// The toolbar's right half: the mode as three icons in one switch,
    /// named in their tooltips.
    pub fn toolbar_modes(&self) -> Element<'_, Message> {
        let mode = |glyph: Glyph, hint: &'static str, mode: Mode| {
            let on = self.mode == mode;
            tip(
                button(icon(glyph, on))
                    .padding([4, 5])
                    .style(move |theme, status| style(theme, status, on))
                    .on_press(Message(Input::Tool(Key::SetMode(mode)))),
                hint,
            )
        };
        let modes = row![
            mode(
                Glyph::Live,
                "Live preview: markdown renders as you type; markers show where the caret is (Ctrl+Shift+E)",
                Mode::Live,
            ),
            mode(
                Glyph::Markdown,
                "Markdown: the text exactly as saved, nothing hidden (Ctrl+Shift+E)",
                Mode::Source,
            ),
            mode(
                Glyph::Split,
                "Side by side: markdown on the left, the rendered note on the right, scrolled together (Ctrl+Shift+E)",
                Mode::Split,
            ),
        ]
        .spacing(2);
        container(modes)
            .padding(2)
            .style(|theme: &Theme| container::Style {
                background: Some(theme.palette().background.weak.color.into()),
                border: Border::default().rounded(9),
                ..container::Style::default()
            })
            .into()
    }
}

/// `content` with `hint` under it on hover; Cmd for Ctrl on macOS.
fn tip<'a>(content: impl Into<Element<'a, Message>>, hint: &'static str) -> Element<'a, Message> {
    let hint = if cfg!(target_os = "macos") {
        hint.replace("Ctrl+", "Cmd+")
    } else {
        hint.to_owned()
    };
    tooltip(
        content,
        container(text(hint).size(12)).padding([4, 8]),
        tooltip::Position::Bottom,
    )
    .gap(6)
    .style(container::rounded_box)
    .into()
}

/// A toolbar button: plain, shaded under the pointer; the mode shown is
/// raised out of its switch, on the page's color with an edge.
fn style(theme: &Theme, status: button::Status, on: bool) -> button::Style {
    let palette = theme.palette();
    let background = if on {
        Some(palette.background.base.color)
    } else {
        match status {
            button::Status::Hovered => Some(palette.background.weak.color),
            button::Status::Pressed => Some(palette.background.strong.color),
            _ => None,
        }
    };
    button::Style {
        background: background.map(Background::Color),
        text_color: palette.background.base.text,
        border: Border {
            color: if on {
                palette.background.strong.color
            } else {
                iced::Color::TRANSPARENT
            },
            width: 1.0,
            radius: 7.0.into(),
        },
        ..button::Style::default()
    }
}
