//! The line between side by side's panes (PLAN-003): dragged, it shares
//! the width out again, each pane keeping at least a fifth; a double click
//! puts it back in the middle.
use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer::{self, Renderer as _};
use iced::advanced::widget::{self, Tree};
use iced::advanced::{Shell, Widget, mouse};
use iced::{Element, Event, Length, Rectangle, Size, Theme};

use super::{Editor, Input, Message};

/// The divider's width: the line and room to grab it.
pub(super) const WIDTH: f32 = 9.0;

/// The least share of the width a pane keeps.
pub(super) const MIN_SHARE: f32 = 0.2;

pub(super) struct Divider<'a> {
    pub editor: &'a Editor,
}

#[derive(Default)]
struct State {
    dragging: bool,
    last_click: Option<mouse::Click>,
}

impl Widget<Message, Theme, iced::Renderer> for Divider<'_> {
    fn tag(&self) -> widget::tree::Tag {
        widget::tree::Tag::of::<State>()
    }

    fn state(&self) -> widget::tree::State {
        widget::tree::State::new(State::default())
    }

    fn size(&self) -> Size<Length> {
        Size::new(Length::Fixed(WIDTH), Length::Fill)
    }

    fn layout(&mut self, tree: &mut Tree, _renderer: &iced::Renderer, limits: &layout::Limits) {
        tree.size = limits.resolve(Length::Fixed(WIDTH), Length::Fill, Size::ZERO);
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout,
        cursor: mouse::Cursor,
        _renderer: &iced::Renderer,
        shell: &mut Shell<'_, Message>,
        _viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_mut::<State>();
        let bounds = layout.bounds();
        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                let Some(position) = cursor.position_over(bounds) else {
                    return;
                };
                let click = mouse::Click::new(position, mouse::Button::Left, state.last_click);
                state.last_click = Some(click);
                if click.kind() == mouse::click::Kind::Double {
                    shell.publish(Message(Input::SplitRatio(0.5)));
                } else {
                    state.dragging = true;
                }
                shell.capture_event();
            }
            Event::Mouse(mouse::Event::CursorMoved { position }) if state.dragging => {
                // The row the panes share: the text's width left of the
                // divider, the rendered pane's right of it.
                let left = self.editor.lines.borrow().outer.width;
                let right = self.editor.preview.lines.borrow().outer.width;
                let start = bounds.x - left;
                let ratio = (position.x - WIDTH / 2.0 - start) / (left + right).max(1.0);
                shell.publish(Message(Input::SplitRatio(ratio)));
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) if state.dragging => {
                state.dragging = false;
            }
            _ => {}
        }
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout,
        cursor: mouse::Cursor,
        _viewport: &Rectangle,
        _renderer: &iced::Renderer,
    ) -> mouse::Interaction {
        let state = tree.state.downcast_ref::<State>();
        if state.dragging || cursor.is_over(layout.bounds()) {
            mouse::Interaction::ResizingColumn
        } else {
            mouse::Interaction::default()
        }
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut iced::Renderer,
        theme: &Theme,
        _style: &renderer::Style,
        layout: Layout,
        cursor: mouse::Cursor,
        _viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_ref::<State>();
        let bounds = layout.bounds();
        let palette = theme.palette();
        // A hairline; stronger while grabbed or under the pointer.
        let (width, color) = if state.dragging || cursor.is_over(bounds) {
            (3.0, palette.primary.base.color)
        } else {
            (1.0, palette.background.strong.color)
        };
        let line = Rectangle::new(
            iced::Point::new(bounds.center_x() - width / 2.0, bounds.y),
            Size::new(width, bounds.height),
        );
        renderer.fill_quad(
            renderer::Quad {
                bounds: line,
                ..renderer::Quad::default()
            },
            color,
        );
    }
}

impl<'a> From<Divider<'a>> for Element<'a, Message> {
    fn from(divider: Divider<'a>) -> Self {
        Element::new(divider)
    }
}
