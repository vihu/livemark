//! The editor widget driven through iced's headless simulator: clicks place
//! the caret, typing and input method commits edit there, keys move and
//! undo, and a long document scrolls to the caret.
use iced::keyboard::{self, Key, key::Named};
use iced::{Event, Point, mouse};
use livemark::widget::{Editor, Message};

const SIZE: (f32, f32) = (800.0, 600.0);

/// Feeds one round of events and applies the messages they produce. Each
/// round starts unfocused (a new widget tree), so rounds that type begin
/// with a click.
fn run(editor: &mut Editor, events: impl FnOnce(&mut iced_test::Simulator<'_, Message>)) {
    let messages: Vec<_> = {
        let mut ui =
            iced_test::Simulator::with_size(iced::Settings::default(), SIZE, editor.view());
        events(&mut ui);
        ui.into_messages().collect()
    };
    for message in messages {
        editor.update(message);
    }
}

fn click(ui: &mut iced_test::Simulator<'_, Message>, at: Point) {
    ui.point_at(at);
    ui.simulate([
        Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
        Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
    ]);
}

fn press(key: Key, modifiers: keyboard::Modifiers) -> Event {
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
fn caret_line(editor: &Editor) -> usize {
    editor.text()[..editor.selection().head]
        .matches('\n')
        .count()
}

#[test]
fn a_click_right_of_a_line_puts_the_caret_at_its_end_and_typing_goes_there() {
    let mut editor = Editor::new("# Title\n\nbody text\nlast\n".into());
    // Padding 16, a level 1 heading line (34 high), an empty line (24):
    // the third line starts 74 down.
    run(&mut editor, |ui| {
        click(ui, Point::new(700.0, 84.0));
        ui.typewrite("!");
    });
    assert_eq!(editor.text(), "# Title\n\nbody text!\nlast\n");
    run(&mut editor, |ui| {
        click(ui, Point::new(700.0, 84.0));
        ui.tap_key(Named::Backspace);
        ui.tap_key(Named::Enter);
    });
    assert_eq!(editor.text(), "# Title\n\nbody text\n\nlast\n");
}

#[test]
fn an_input_method_commit_inserts_at_the_caret() {
    let mut editor = Editor::new("ab".into());
    run(&mut editor, |ui| {
        click(ui, Point::new(700.0, 20.0));
        ui.simulate([Event::InputMethod(
            iced::advanced::input_method::Event::Preedit("にほ".into(), None),
        )]);
        ui.simulate([Event::InputMethod(
            iced::advanced::input_method::Event::Commit("日本".into()),
        )]);
    });
    assert_eq!(
        editor.text(),
        "ab日本",
        "the preedit never reaches the text"
    );
}

#[test]
fn arrows_move_and_ctrl_z_undoes_a_typing_burst() {
    let mut editor = Editor::new("abc\nabc".into());
    run(&mut editor, |ui| {
        click(ui, Point::new(700.0, 20.0));
        ui.tap_key(Named::ArrowDown);
        ui.typewrite("XY");
    });
    assert_eq!(editor.text(), "abc\nabcXY", "down keeps the x, at the end");
    run(&mut editor, |ui| {
        click(ui, Point::new(700.0, 20.0));
        ui.simulate([press(
            Key::Character("z".into()),
            keyboard::Modifiers::COMMAND,
        )]);
    });
    assert_eq!(editor.text(), "abc\nabc");
    run(&mut editor, |ui| {
        click(ui, Point::new(700.0, 20.0));
        let redo = keyboard::Modifiers::COMMAND | keyboard::Modifiers::SHIFT;
        ui.simulate([press(Key::Character("z".into()), redo)]);
    });
    assert_eq!(editor.text(), "abc\nabcXY", "Ctrl+Shift+Z redoes");
}

#[test]
fn ctrl_end_in_a_long_document_scrolls_to_the_caret() {
    let coverage = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/docs/coverage.md"
    ))
    .unwrap();
    let text = coverage.repeat(5_000 / coverage.lines().count() + 1);
    let lines = text.lines().count();
    let mut editor = Editor::new(text);
    run(&mut editor, |ui| {
        click(ui, Point::new(30.0, 30.0));
        ui.simulate([press(Key::Named(Named::End), keyboard::Modifiers::COMMAND)]);
    });
    assert_eq!(editor.selection().head, editor.text().len());
    // A click near the top now lands within the last screen of lines.
    run(&mut editor, |ui| click(ui, Point::new(30.0, 30.0)));
    let line = caret_line(&editor);
    assert!(line + 40 > lines && line < lines, "line {line} of {lines}");
    // And the wheel scrolls back up.
    run(&mut editor, |ui| {
        ui.point_at(Point::new(30.0, 30.0));
        ui.simulate([Event::Mouse(mouse::Event::WheelScrolled {
            delta: mouse::ScrollDelta::Lines { x: 0.0, y: 10.0 },
        })]);
        click(ui, Point::new(30.0, 30.0));
    });
    assert!(caret_line(&editor) + 10 < line, "scrolled up from {line}");
}

#[test]
fn the_editor_draws_headlessly() {
    let coverage = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/docs/coverage.md"
    ))
    .unwrap();
    let mut editor = Editor::new(coverage);
    editor.select(3, 3);
    for theme in [iced::Theme::Light, iced::Theme::Dark] {
        let mut ui =
            iced_test::Simulator::with_size(iced::Settings::default(), SIZE, editor.view());
        ui.snapshot(&theme).expect("a frame");
    }
}

#[test]
fn ctrl_with_a_letter_is_left_to_the_host() {
    let mut editor = Editor::new("note".into());
    let mut statuses = Vec::new();
    run(&mut editor, |ui| {
        click(ui, Point::new(700.0, 20.0));
        // Some platforms report the letter as text even with Ctrl held.
        let mut save = press(Key::Character("s".into()), keyboard::Modifiers::COMMAND);
        if let Event::Keyboard(keyboard::Event::KeyPressed { text, .. }) = &mut save {
            *text = Some("s".into());
        }
        statuses = ui.simulate([save]);
    });
    assert_eq!(editor.text(), "note");
    assert_eq!(
        statuses,
        [iced::event::Status::Ignored],
        "the app sees Ctrl+S"
    );
}
