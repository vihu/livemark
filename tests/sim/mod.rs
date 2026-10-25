//! What the simulator tests share: a round of events through iced's
//! headless simulator, a click, a key press, and what the caret is on.
use iced::keyboard::{self, Key};
use iced::{Event, Point, mouse};
use livemark::widget::{Editor, Message};

pub const SIZE: (f32, f32) = (800.0, 600.0);

/// Feeds one round of events and applies the messages they produce. Each
/// round starts unfocused (a new widget tree), so rounds that type begin
/// with a click.
pub fn run(editor: &mut Editor, events: impl FnOnce(&mut iced_test::Simulator<'_, Message>)) {
    let messages: Vec<_> = {
        let mut ui =
            iced_test::Simulator::with_size(iced::Settings::default(), SIZE, editor.view());
        events(&mut ui);
        ui.into_messages().collect()
    };
    for message in messages {
        let _ = editor.update(message);
    }
}

pub fn click(ui: &mut iced_test::Simulator<'_, Message>, at: Point) {
    ui.point_at(at);
    ui.simulate([
        Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
        Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
    ]);
}

pub fn press(key: Key, modifiers: keyboard::Modifiers) -> Event {
    Event::Keyboard(keyboard::Event::KeyPressed {
        key: key.clone(),
        modified_key: key,
        physical_key: keyboard::key::Physical::Unidentified(
            keyboard::key::NativeCode::Unidentified,
        ),
        location: keyboard::Location::Standard,
        modifiers,
        text: None,
        repeat: false,
    })
}

/// The line the caret is on.
pub fn caret_line(editor: &Editor) -> usize {
    editor.text()[..editor.selection().head]
        .matches('\n')
        .count()
}

/// The selected text.
pub fn selected(editor: &Editor) -> &str {
    &editor.text()[editor.selection().range()]
}
