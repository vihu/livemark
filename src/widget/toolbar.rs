//! The toolbar (PLAN-002): formatting, line prefixes and the mode, each
//! button running its key's command and giving the keyboard back to the
//! text. A host shows it where it likes, with its own items around it.
use iced::widget::{Space, button, row, text, tooltip};
use iced::{Element, Font, font};

use super::{Editor, Input, Key, Message, Mode};
use crate::edit::blocks::Block;
use crate::edit::format::Format;

impl Editor {
    /// The toolbar: bold, italic, code, link; heading, bullet, task, quote;
    /// live, source and split mode. Opt-in: nothing shows it unless the host puts
    /// it in its view.
    pub fn toolbar(&self) -> Element<'_, Message> {
        let mode = self.mode;
        let tool = |label: Element<'static, Message>, hint: &'static str, key: Key, on: bool| {
            let style = if on { button::primary } else { button::text };
            tooltip(
                button(label)
                    .style(style)
                    .padding([4, 10])
                    .on_press(Message(Input::Tool(key))),
                text(hint).size(12),
                tooltip::Position::Bottom,
            )
            .gap(4)
            .style(iced::widget::container::rounded_box)
        };
        let label = |content: &'static str, font: Font| text(content).size(14).font(font).into();
        let bold = Font {
            weight: font::Weight::Bold,
            ..Font::DEFAULT
        };
        let italic = Font {
            style: font::Style::Italic,
            ..Font::DEFAULT
        };
        let gap = || Space::new().width(12);
        row![
            tool(
                label("B", bold),
                "Bold (Ctrl+B)",
                Key::Format(Format::Bold),
                false
            ),
            tool(
                label("I", italic),
                "Italic (Ctrl+I)",
                Key::Format(Format::Italic),
                false
            ),
            tool(
                label("Code", Font::MONOSPACE),
                "Code (Ctrl+E)",
                Key::Format(Format::Code),
                false
            ),
            tool(
                label("Link", Font::DEFAULT),
                "Link (Ctrl+K)",
                Key::Link,
                false
            ),
            gap(),
            tool(
                label("H", bold),
                "Heading: #, ##, ###, none",
                Key::Block(Block::Heading),
                false
            ),
            tool(
                label("List", Font::DEFAULT),
                "Bullet list",
                Key::Block(Block::Bullet),
                false
            ),
            tool(
                label("Task", Font::DEFAULT),
                "Task list",
                Key::Block(Block::Task),
                false
            ),
            tool(
                label("Quote", Font::DEFAULT),
                "Quote",
                Key::Block(Block::Quote),
                false
            ),
            gap(),
            tool(
                label("Live", Font::DEFAULT),
                "Live preview (Ctrl+Shift+E)",
                Key::SetMode(Mode::Live),
                mode == Mode::Live,
            ),
            tool(
                label("Source", Font::DEFAULT),
                "The markdown as written (Ctrl+Shift+E)",
                Key::SetMode(Mode::Source),
                mode == Mode::Source,
            ),
            tool(
                label("Split", Font::DEFAULT),
                "The markdown beside the rendered note (Ctrl+Shift+E)",
                Key::SetMode(Mode::Split),
                mode == Mode::Split,
            ),
        ]
        .spacing(2)
        .align_y(iced::Center)
        .into()
    }
}
