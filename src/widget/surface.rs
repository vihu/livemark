//! The iced widget over an [`Editor`]: turns events into messages, keeps
//! focus, the blinking caret and the input method's preedit, and draws.
//! Side by side's rendered pane is one too, taking presses, the wheel and
//! its scroll bar, never focus or keys.
//! Event handling follows iced's `text_editor` at the pinned rev
//! (`core/src/text/editor.rs`).
use std::time::{Duration, Instant};

use iced::advanced::layout::{self, Layout};
use iced::advanced::widget::operation::Focusable;
use iced::advanced::widget::{self, Operation, Tree};
use iced::advanced::{Renderer as _, Shell, Widget, clipboard, input_method, mouse};
use iced::keyboard;
use iced::widget::text_editor::{Binding, KeyPress, Motion as IcedMotion};
use iced::{Element, Event, Length, Pixels, Point, Rectangle, Size, Theme, Vector, window};

use super::find::FindInput;
use super::shape::TEXT_SIZE;
use super::{Editor, ID, Input, Key, Message, Pane, Vertical, keys, scrollbar};
use crate::edit::Motion;

/// Space between the widget's edge and the text.
const PADDING: f32 = 16.0;

/// The gutter left of the text, at 100%, where a revealed heading's `#`
/// run hangs (REFERENCE-001 section 3): with the padding, room for `### `
/// at its size; a wider run (`#### ` and on) pushes its text right by the
/// rest.
const GUTTER: f32 = 2.25 * TEXT_SIZE;

/// What a hanging `#` run keeps clear of the widget's left edge.
pub(super) const EDGE: f32 = 4.0;

/// Where the text goes in a widget's `bounds` at `zoom`: inside the
/// padding, right of the gutter (which grows with the zoom).
pub(super) fn text_area(bounds: Rectangle, zoom: f32) -> Rectangle {
    let gutter = GUTTER * zoom;
    Rectangle::new(
        Point::new(bounds.x + PADDING + gutter, bounds.y + PADDING),
        Size::new(
            (bounds.width - 2.0 * PADDING - gutter).max(1.0),
            (bounds.height - 2.0 * PADDING).max(1.0),
        ),
    )
}

/// How long the caret stays on, then off.
const BLINK_MILLIS: u128 = 500;

/// Lines one wheel notch scrolls, in body line heights.
const WHEEL_LINES: f32 = 3.0;

pub(super) struct Surface<'a> {
    pub editor: &'a Editor,
    pub pane: Pane,
}

#[derive(Default)]
struct State {
    focus: Option<Focus>,
    last_click: Option<mouse::Click>,
    dragging: bool,
    preedit: Option<input_method::Preedit>,
    /// A paste asked the clipboard for this: its text, then, when it has
    /// none, a picture.
    pasting: Option<clipboard::Kind>,
    modifiers: keyboard::Modifiers,
    /// The scroll bar's thumb is held this far below its top.
    thumb_grab: Option<f32>,
}

struct Focus {
    updated_at: Instant,
    now: Instant,
    window_focused: bool,
}

impl Focus {
    fn now() -> Self {
        let now = Instant::now();
        Self {
            updated_at: now,
            now,
            window_focused: true,
        }
    }

    fn caret_on(&self) -> bool {
        self.window_focused
            && ((self.now - self.updated_at).as_millis() / BLINK_MILLIS).is_multiple_of(2)
    }
}

impl Focusable for State {
    fn is_focused(&self) -> bool {
        self.focus.is_some()
    }

    fn focus(&mut self) {
        self.focus = Some(Focus::now());
    }

    fn unfocus(&mut self) {
        self.focus = None;
    }
}

impl Widget<Message, Theme, iced::Renderer> for Surface<'_> {
    fn tag(&self) -> widget::tree::Tag {
        widget::tree::Tag::of::<State>()
    }

    fn state(&self) -> widget::tree::State {
        widget::tree::State::new(State::default())
    }

    fn size(&self) -> Size<Length> {
        Size::new(self.width(), Length::Fill)
    }

    fn layout(&mut self, tree: &mut Tree, _renderer: &iced::Renderer, limits: &layout::Limits) {
        let size = limits.resolve(self.width(), Length::Fill, Size::ZERO);
        let lines = match self.pane {
            Pane::Text => &self.editor.lines,
            Pane::Preview => &self.editor.preview.lines,
        };
        let pending = {
            let mut lines = lines.borrow_mut();
            lines.outer = size;
            lines.fit();
            lines.sized = true;
            lines.pending_reveal.take()
        };
        // A caret selected before the first layout, now that the size is
        // known.
        if let Some((offset, side)) = pending {
            let len = self.editor.text().len();
            self.editor
                .with_lines(|lines, source| lines.reveal(source, offset.min(len), side));
            self.editor.preview.leader.set(Pane::Text);
        }
        // Side by side: where the other pane is, if this one follows it.
        if self.editor.preview.leader.get() != self.pane {
            self.editor.follow();
        }
        tree.size = size;
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout,
        _viewport: &Rectangle,
        _renderer: &iced::Renderer,
        operation: &mut dyn Operation,
    ) {
        let state = tree.state.downcast_mut::<State>();
        if self.pane == Pane::Text {
            operation.focusable(Some(&ID), layout.bounds(), state);
        }
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
        let text = self.text_area(bounds).position();
        let publish = |shell: &mut Shell<'_, Message>, input| shell.publish(Message(input));
        match event {
            Event::Window(window::Event::Unfocused) => {
                if let Some(focus) = &mut state.focus {
                    focus.window_focused = false;
                }
            }
            Event::Window(window::Event::Focused) => {
                if let Some(focus) = &mut state.focus {
                    focus.window_focused = true;
                    focus.updated_at = Instant::now();
                    shell.request_redraw();
                }
            }
            Event::Window(window::Event::RedrawRequested(now)) => {
                let Some(focus) = state.focus.as_mut().filter(|f| f.window_focused) else {
                    return;
                };
                focus.now = *now;
                let left = BLINK_MILLIS - (focus.now - focus.updated_at).as_millis() % BLINK_MILLIS;
                shell.request_redraw_at(*now + Duration::from_millis(left as u64));
                let caret = self.editor.caret().unwrap_or(Rectangle::new(
                    Point::ORIGIN,
                    Size::new(1.0, TEXT_SIZE * self.editor.zoom()),
                ));
                shell.request_input_method(&input_method::InputMethod::Enabled {
                    cursor: caret + Vector::new(text.x, text.y),
                    purpose: input_method::Purpose::Normal,
                    preedit: state.preedit.as_ref().map(input_method::Preedit::as_ref),
                });
            }
            Event::Keyboard(keyboard::Event::ModifiersChanged(modifiers)) => {
                if modifiers.shift() != state.modifiers.shift() && self.pane == Pane::Text {
                    publish(shell, Input::Shift(modifiers.shift()));
                }
                state.modifiers = *modifiers;
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
                if self.thumb_at(bounds, cursor).is_some() =>
            {
                // The scroll bar: hold the thumb where it was pressed, or
                // move it under the pointer when the track was pressed.
                let (track, thumb, y) = self.thumb_at(bounds, cursor).expect("checked");
                let grab = if y >= thumb.y && y <= thumb.y + thumb.height {
                    y - thumb.y
                } else {
                    thumb.height / 2.0
                };
                state.thumb_grab = Some(grab);
                let t = scrollbar::travel(track, thumb, y - grab);
                publish(shell, Input::ScrollTo(self.pane, t));
                shell.capture_event();
            }
            Event::Mouse(mouse::Event::CursorMoved { position }) if state.thumb_grab.is_some() => {
                let track = scrollbar::track(bounds);
                if let Some(thumb) = self.editor.thumb(self.pane, track) {
                    let grab = state.thumb_grab.unwrap_or(0.0);
                    let t = scrollbar::travel(track, thumb, position.y - grab);
                    publish(shell, Input::ScrollTo(self.pane, t));
                }
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))
                if state.thumb_grab.is_some() =>
            {
                state.thumb_grab = None;
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
                if self.pane == Pane::Preview =>
            {
                if let Some(position) = cursor.position_over(bounds) {
                    let at = position - Vector::new(text.x, text.y);
                    let command = state.modifiers.command();
                    publish(shell, Input::PreviewPress { at, command });
                    shell.capture_event();
                }
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                if let Some(position) = cursor.position_over(bounds) {
                    let click = mouse::Click::new(position, mouse::Button::Left, state.last_click);
                    state.last_click = Some(click);
                    state.focus = Some(Focus::now());
                    state.dragging = true;
                    let at = position - Vector::new(text.x, text.y);
                    let shift = state.modifiers.shift();
                    let clicks = match click.kind() {
                        mouse::click::Kind::Single => 1,
                        mouse::click::Kind::Double => 2,
                        mouse::click::Kind::Triple => 3,
                    };
                    let command = state.modifiers.command();
                    let rest = keyboard::Modifiers::SHIFT | keyboard::Modifiers::COMMAND;
                    let other = !state.modifiers.difference(rest).is_empty();
                    publish(
                        shell,
                        Input::Press {
                            at,
                            shift,
                            clicks,
                            command,
                            other,
                        },
                    );
                    shell.capture_event();
                } else if state.focus.take().is_some() {
                    shell.request_redraw();
                }
            }
            Event::Mouse(mouse::Event::CursorMoved { position }) if state.dragging => {
                let at = *position - Vector::new(text.x, text.y);
                publish(shell, Input::Drag(at));
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) if state.dragging => {
                state.dragging = false;
                publish(shell, Input::Release);
            }
            Event::Mouse(mouse::Event::WheelScrolled { delta }) if cursor.is_over(bounds) => {
                // With Ctrl/Cmd, zoom: a tenth a notch, as the app's keys
                // step; a touchpad's pixels zoom smoothly.
                if state.modifiers.command() {
                    let zoom = match delta {
                        // Not `signum`: it is 1 for a sideways scroll's 0.
                        mouse::ScrollDelta::Lines { y, .. } => {
                            Input::ZoomSteps(f32::from(i8::from(*y > 0.0) - i8::from(*y < 0.0)))
                        }
                        mouse::ScrollDelta::Pixels { y, .. } => Input::ZoomBy(1.0 + y * 0.002),
                    };
                    publish(shell, zoom);
                    shell.capture_event();
                    return;
                }
                let dy = match delta {
                    mouse::ScrollDelta::Lines { y, .. } => {
                        -y * WHEEL_LINES * TEXT_SIZE * self.editor.zoom() * 1.5
                    }
                    mouse::ScrollDelta::Pixels { y, .. } => -y,
                };
                publish(shell, Input::Scroll(self.pane, dy));
                shell.capture_event();
            }
            Event::InputMethod(event) if state.focus.is_some() => {
                match event {
                    input_method::Event::Opened | input_method::Event::Closed => {
                        state.preedit = matches!(event, input_method::Event::Opened)
                            .then(input_method::Preedit::new);
                    }
                    input_method::Event::Preedit(content, selection) => {
                        state.preedit = Some(input_method::Preedit {
                            content: content.clone(),
                            selection: selection.clone(),
                            text_size: Some(Pixels(TEXT_SIZE * self.editor.zoom())),
                        });
                    }
                    input_method::Event::Commit(content) => {
                        publish(shell, Input::Commit(content.clone()));
                    }
                }
                shell.request_redraw();
                shell.capture_event();
            }
            // No text to paste: a picture, perhaps (PLAN-002); nothing at
            // all ends the read, so a later one (another widget's) is not
            // taken.
            Event::Clipboard(clipboard::Event::Read(Err(_))) if state.pasting.is_some() => {
                if state.pasting == Some(clipboard::Kind::Text) {
                    state.pasting = Some(clipboard::Kind::Image);
                    shell.read_clipboard(clipboard::Kind::Image);
                } else {
                    state.pasting = None;
                }
            }
            // Only the kind it asked for: another widget's read of another
            // kind is not taken.
            Event::Clipboard(clipboard::Event::Read(Ok(content))) if state.pasting.is_some() => {
                match (state.pasting, content.as_ref()) {
                    (Some(clipboard::Kind::Text), clipboard::Content::Text(text)) => {
                        state.pasting = None;
                        publish(shell, Input::Paste(text.clone()));
                    }
                    (Some(clipboard::Kind::Image), clipboard::Content::Image(image)) => {
                        state.pasting = None;
                        publish(shell, Input::PastedImage(image.clone()));
                    }
                    _ => {}
                }
            }
            Event::Keyboard(keyboard::Event::KeyPressed {
                key,
                modified_key,
                physical_key,
                modifiers,
                text,
                ..
            }) if self.pane == Pane::Text
                && (state.focus.is_some() || self.editor.find.is_some()) =>
            {
                // The find bar's keys, also while its fields have focus
                // (REFERENCE-001 section 16).
                if let Some(input) = self.find_key(key, *physical_key, *modifiers) {
                    publish(shell, Input::Find(input));
                    shell.capture_event();
                    return;
                }
                if state.focus.is_none() {
                    // A find bar field has focus: Tab goes to the next.
                    if *key == keyboard::Key::Named(keyboard::key::Named::Tab)
                        && !modifiers.command()
                        && !modifiers.alt()
                    {
                        let forward = !modifiers.shift();
                        publish(shell, Input::Find(FindInput::Tab { forward }));
                        shell.capture_event();
                    }
                    return;
                }
                let press = KeyPress {
                    key: key.clone(),
                    modified_key: modified_key.clone(),
                    physical_key: *physical_key,
                    modifiers: *modifiers,
                    text: text.clone(),
                    is_focused: true,
                };
                // Tab types nothing in iced's bindings (its text is a
                // control character); here it indents (REFERENCE-001
                // section 7).
                if *key == keyboard::Key::Named(keyboard::key::Named::Tab) && !modifiers.command() {
                    publish(shell, Input::Key(Key::Indent(modifiers.shift())));
                    if let Some(focus) = &mut state.focus {
                        focus.updated_at = Instant::now();
                    }
                    shell.capture_event();
                    return;
                }
                if let Some(shortcut) = keys::shortcut(key, *physical_key, *modifiers) {
                    publish(shell, Input::Key(shortcut));
                    if let Some(focus) = &mut state.focus {
                        focus.updated_at = Instant::now();
                    }
                    shell.capture_event();
                    return;
                }
                let Some(binding) = Binding::<()>::from_key_press(press) else {
                    return;
                };
                // Ctrl/Cmd with a letter is the host's shortcut (Ctrl+S),
                // not typing; AltGr letters come with Alt and still type.
                if matches!(binding, Binding::Insert(_)) && modifiers.command() && !modifiers.alt()
                {
                    return;
                }
                self.bind(binding, modifiers.shift(), state, shell);
                if let Some(focus) = &mut state.focus {
                    focus.updated_at = Instant::now();
                }
                shell.capture_event();
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
        let text = self.text_area(layout.bounds());
        let on_link = state.modifiers.command()
            && cursor.position_over(text).is_some_and(|at| {
                let at = at - Vector::new(text.x, text.y);
                self.editor.link_under(self.pane, at).is_some()
            });
        if self.thumb_at(layout.bounds(), cursor).is_some() {
            mouse::Interaction::default()
        } else if on_link {
            mouse::Interaction::Pointer
        } else if cursor.is_over(layout.bounds()) && self.pane == Pane::Text {
            mouse::Interaction::Text
        } else {
            mouse::Interaction::default()
        }
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut iced::Renderer,
        theme: &Theme,
        _style: &iced::advanced::renderer::Style,
        layout: Layout,
        _cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_ref::<State>();
        let caret = state.focus.as_ref().is_some_and(Focus::caret_on);
        // Everything clipped to the text's rows (the side padding stays
        // open for code bands and the quote bar) and to what the parent
        // shows: a layer's clip does not narrow its parent's.
        let bounds = layout.bounds();
        let area = self.text_area(bounds);
        let rows = Rectangle::new(
            Point::new(bounds.x, area.y),
            Size::new(bounds.width, area.height),
        );
        if let Some(rows) = rows.intersection(viewport) {
            renderer.with_layer(rows, |renderer| {
                self.editor.draw(renderer, theme, area, self.pane, caret);
            });
        }
        // Over the text's layer, not under it.
        if let Some(shown) = bounds.intersection(viewport) {
            renderer.with_layer(shown, |renderer| {
                self.editor
                    .draw_scrollbar(renderer, theme, self.pane, bounds);
            });
        }
    }
}

impl Surface<'_> {
    /// Its share of the row: in Split, the text's split ratio (in
    /// thousandths) or the rest.
    fn width(&self) -> Length {
        let text = (self.editor.split_ratio * 1000.0).round() as u16;
        match self.pane {
            _ if self.editor.mode != super::Mode::Split => Length::Fill,
            Pane::Text => Length::FillPortion(text),
            Pane::Preview => Length::FillPortion(1000 - text),
        }
    }

    /// Where the text goes in the widget's `bounds`.
    fn text_area(&self, bounds: Rectangle) -> Rectangle {
        text_area(bounds, self.editor.zoom())
    }

    /// The scroll bar's track and thumb when the pointer is over the track
    /// (a little wider, to be easy to hit), with the pointer's y.
    fn thumb_at(
        &self,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<(Rectangle, Rectangle, f32)> {
        let position = cursor.position_over(bounds)?;
        let track = scrollbar::track(bounds);
        let near = position.x >= track.x - 4.0
            && position.y >= track.y
            && position.y <= track.y + track.height;
        let thumb = self.editor.thumb(self.pane, track).filter(|_| near)?;
        Some((track, thumb, position.y))
    }

    /// Ctrl/Cmd+F and Ctrl/Cmd+H open the find bar; while it is open, F3
    /// and Ctrl/Cmd+G go to the next match (Shift: the previous one) and
    /// Escape closes it.
    fn find_key(
        &self,
        key: &keyboard::Key,
        physical_key: keyboard::key::Physical,
        modifiers: keyboard::Modifiers,
    ) -> Option<FindInput> {
        let letter = key.to_latin(physical_key);
        let open = self.editor.find.is_some();
        let forward = !modifiers.shift();
        match key.as_ref() {
            _ if modifiers.command() && !modifiers.alt() && matches!(letter, Some('f' | 'h')) => {
                Some(FindInput::Open {
                    replace: letter == Some('h'),
                })
            }
            keyboard::Key::Named(keyboard::key::Named::F3) if open => {
                Some(FindInput::Step { forward })
            }
            _ if open && modifiers.command() && letter == Some('g') => {
                Some(FindInput::Step { forward })
            }
            keyboard::Key::Named(keyboard::key::Named::Escape) if open => Some(FindInput::Close),
            _ => None,
        }
    }

    /// Carries out a key binding: clipboard ones here, the rest as
    /// messages.
    fn bind(
        &self,
        binding: Binding<()>,
        shift: bool,
        state: &mut State,
        shell: &mut Shell<'_, Message>,
    ) {
        let key = |key| Message(Input::Key(key));
        match binding {
            Binding::Copy => shell.publish(key(Key::Copy(false))),
            Binding::Cut => shell.publish(key(Key::Copy(true))),
            Binding::Paste => {
                state.pasting = Some(clipboard::Kind::Text);
                shell.read_clipboard(clipboard::Kind::Text);
            }
            // iced binds only Ctrl+Y to redo; REFERENCE-001 section 15 adds
            // Ctrl+Shift+Z.
            Binding::Undo if shift => shell.publish(key(Key::Redo)),
            Binding::Undo => shell.publish(key(Key::Undo)),
            Binding::Redo => shell.publish(key(Key::Redo)),
            Binding::SelectAll => shell.publish(key(Key::SelectAll)),
            Binding::Insert(c) => shell.publish(key(Key::Insert(c))),
            Binding::Enter if shift => shell.publish(key(Key::SoftEnter)),
            Binding::Enter => shell.publish(key(Key::Enter)),
            Binding::Backspace => shell.publish(key(Key::Delete(Motion::Left))),
            Binding::BackspaceWord => shell.publish(key(Key::Delete(Motion::WordLeft))),
            Binding::BackspaceLine => shell.publish(key(Key::Delete(Motion::LineStart))),
            Binding::Delete => shell.publish(key(Key::Delete(Motion::Right))),
            Binding::DeleteWord => shell.publish(key(Key::Delete(Motion::WordRight))),
            Binding::DeleteLine => shell.publish(key(Key::Delete(Motion::LineEnd))),
            Binding::Unfocus => shell.publish(key(Key::Collapse)),
            Binding::Move(motion) | Binding::Select(motion) => {
                let extend = matches!(binding, Binding::Select(_));
                shell.publish(key(motion_key(motion, extend)));
            }
            Binding::Sequence(bindings) => {
                for binding in bindings {
                    self.bind(binding, shift, state, shell);
                }
            }
            Binding::SelectWord | Binding::SelectLine | Binding::Custom(()) => {}
        }
    }
}

fn motion_key(motion: IcedMotion, extend: bool) -> Key {
    let line = |motion| Key::Move(motion, extend);
    match motion {
        IcedMotion::Left => line(Motion::Left),
        IcedMotion::Right => line(Motion::Right),
        IcedMotion::WordLeft => line(Motion::WordLeft),
        IcedMotion::WordRight => line(Motion::WordRight),
        IcedMotion::Home => line(Motion::LineStart),
        IcedMotion::End => line(Motion::LineEnd),
        IcedMotion::DocumentStart => line(Motion::DocumentStart),
        IcedMotion::DocumentEnd => line(Motion::DocumentEnd),
        IcedMotion::Up => Key::Vertical(Vertical::Up, extend),
        IcedMotion::Down => Key::Vertical(Vertical::Down, extend),
        IcedMotion::PageUp => Key::Vertical(Vertical::PageUp, extend),
        IcedMotion::PageDown => Key::Vertical(Vertical::PageDown, extend),
    }
}

impl<'a> From<Surface<'a>> for Element<'a, Message> {
    fn from(surface: Surface<'a>) -> Self {
        Element::new(surface)
    }
}
