//! Completion through iced's headless simulator (PLAN-004): typed `[[`,
//! the host's choices, then Down and Enter on the keyboard put the second
//! choice in; while the list is up, those keys do not move the caret.
use iced::keyboard::{self, Key, key::Named};
use iced::{Event, Point, mouse};
use livemark::widget::{Choice, Complete, Editor, Message};

const SIZE: (f32, f32) = (800.0, 600.0);

fn run(editor: &mut Editor, events: impl FnOnce(&mut iced_test::Simulator<'_, Message>)) {
    let messages: Vec<_> = {
        let mut ui =
            iced_test::Simulator::with_size(iced::Settings::default(), SIZE, editor.view());
        // Focus the text, the caret at the end of the first line.
        ui.point_at(Point::new(700.0, 20.0));
        ui.simulate([
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
        ]);
        events(&mut ui);
        ui.into_messages().collect()
    };
    for message in messages {
        let _ = editor.update(message);
    }
}

fn key(named: Named) -> Event {
    Event::Keyboard(keyboard::Event::KeyPressed {
        key: Key::Named(named),
        modified_key: Key::Named(named),
        physical_key: keyboard::key::Physical::Unidentified(
            keyboard::key::NativeCode::Unidentified,
        ),
        location: keyboard::Location::Standard,
        modifiers: keyboard::Modifiers::empty(),
        text: None,
        repeat: false,
    })
}

#[test]
fn down_and_enter_choose_from_the_list() {
    let mut editor = Editor::new("see ".into());
    run(&mut editor, |ui| {
        ui.typewrite("[[li");
    });
    let completing = editor.completing().expect("completing");
    assert_eq!(
        (completing.kind, completing.query.as_str()),
        (Complete::Link, "li")
    );
    let choice = |label: &str| Choice {
        label: label.into(),
        detail: String::new(),
        insert: format!("[{label}]({}.md)", label.to_lowercase()),
    };
    editor.set_choices(vec![choice("Lima"), choice("Lisbon")]);
    run(&mut editor, |ui| {
        ui.simulate([key(Named::ArrowDown), key(Named::Enter)]);
    });
    assert_eq!(editor.text(), "see [Lisbon](lisbon.md)");
}
