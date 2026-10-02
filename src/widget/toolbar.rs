//! The toolbar (PLAN-003): formatting and line prefixes as icons in two
//! groups, and the modes as a switch at the right edge, named and
//! explained in their tooltips. Each button runs its key's command and
//! gives the keyboard back to the text. A host shows it where it likes,
//! with its own items around it.
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
        let tool = |glyph: Glyph, hint: &'static str, key: Key| {
            tip(
                button(icon(glyph, false))
                    .padding(5)
                    .style(|theme, status| style(theme, status, false))
                    .on_press(Message(Input::Tool(key))),
                hint,
            )
        };
        let format = group(vec![
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
        ]);
        let blocks = group(vec![
            tool(
                Glyph::Heading,
                "Heading: #, ##, ###, then none",
                Key::Block(Block::Heading),
            ),
            tool(Glyph::List, "Bullet list", Key::Block(Block::Bullet)),
            tool(Glyph::Task, "Task list", Key::Block(Block::Task)),
            tool(Glyph::Quote, "Quote", Key::Block(Block::Quote)),
        ]);
        let mode = |glyph: Glyph, label: &'static str, hint: &'static str, mode: Mode| {
            let on = self.mode == mode;
            tip(
                button(
                    row![icon(glyph, on), text(label).size(13)]
                        .spacing(6)
                        .align_y(iced::Center),
                )
                .padding([5, 10])
                .style(move |theme, status| style(theme, status, on))
                .on_press(Message(Input::Tool(Key::SetMode(mode)))),
                hint,
            )
        };
        let modes = group(vec![
            mode(
                Glyph::Live,
                "Live preview",
                "Markdown renders as you type; markers show where the caret is (Ctrl+Shift+E)",
                Mode::Live,
            ),
            mode(
                Glyph::Markdown,
                "Markdown",
                "The text exactly as saved, nothing hidden (Ctrl+Shift+E)",
                Mode::Source,
            ),
            mode(
                Glyph::Split,
                "Side by side",
                "Markdown on the left, the rendered note on the right, scrolled together (Ctrl+Shift+E)",
                Mode::Split,
            ),
        ]);
        row![format, blocks, Space::new().width(Length::Fill), modes]
            .spacing(8)
            .width(Length::Fill)
            .align_y(iced::Center)
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

/// Buttons side by side in one rounded outline.
fn group(buttons: Vec<Element<'_, Message>>) -> Element<'_, Message> {
    container(row(buttons).spacing(2))
        .padding(2)
        .style(|theme: &Theme| container::Style {
            border: Border {
                color: theme.palette().background.strong.color,
                width: 1.0,
                radius: 7.0.into(),
            },
            ..container::Style::default()
        })
        .into()
}

/// A toolbar button: plain, shaded under the pointer, filled when it is
/// the mode shown.
fn style(theme: &Theme, status: button::Status, on: bool) -> button::Style {
    let palette = theme.palette();
    let (background, text_color) = if on {
        (Some(palette.primary.base.color), palette.primary.base.text)
    } else {
        let shade = match status {
            button::Status::Hovered => Some(palette.background.weak.color),
            button::Status::Pressed => Some(palette.background.strong.color),
            _ => None,
        };
        (shade, palette.background.base.text)
    };
    button::Style {
        background: background.map(Background::Color),
        text_color,
        border: Border {
            radius: 5.0.into(),
            ..Border::default()
        },
        ..button::Style::default()
    }
}
