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
        let _ = editor.update(message);
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

/// The selected text.
fn selected(editor: &Editor) -> &str {
    &editor.text()[editor.selection().range()]
}

#[test]
fn double_and_triple_clicks_select_a_word_and_a_line() {
    let mut editor = Editor::new("alpha beta gamma\nsecond line\n".into());
    // 16 px text: "alpha" covers well past 8 px into the text area.
    let word = Point::new(16.0 + 8.0, 20.0);
    run(&mut editor, |ui| {
        click(ui, word);
        click(ui, word);
    });
    assert_eq!(selected(&editor), "alpha");
    run(&mut editor, |ui| {
        // A drag on the second press of a double click grows by words, to
        // the line's end.
        click(ui, word);
        ui.simulate([Event::Mouse(mouse::Event::ButtonPressed(
            mouse::Button::Left,
        ))]);
        let end = Point::new(700.0, 20.0);
        ui.point_at(end);
        ui.simulate([Event::Mouse(mouse::Event::CursorMoved { position: end })]);
    });
    assert_eq!(selected(&editor), "alpha beta gamma");
    run(&mut editor, |ui| {
        let line = Point::new(30.0, 16.0 + 24.0 + 10.0);
        for _ in 0..3 {
            click(ui, line);
        }
    });
    assert_eq!(selected(&editor), "second line\n", "with its ending");
}

#[test]
fn home_and_end_stop_at_the_edges_of_a_wrapped_row_first() {
    let text = "word ".repeat(40);
    let mut editor = Editor::new(text.clone());
    let narrow = |editor: &mut Editor, keys: &[Event]| {
        let messages: Vec<_> = {
            let mut ui = iced_test::Simulator::with_size(
                iced::Settings::default(),
                (300.0, 600.0),
                editor.view(),
            );
            click(&mut ui, Point::new(20.0, 20.0));
            ui.simulate(keys.iter().cloned());
            ui.into_messages().collect()
        };
        for message in messages {
            let _ = editor.update(message);
        }
        editor.selection().head
    };
    let none = keyboard::Modifiers::default();
    let home = press(Key::Named(Named::Home), none);
    let end = press(Key::Named(Named::End), none);
    // From the start of the first row: End reaches its end, a second End
    // the end of the line (wrapped over at least three rows).
    let row_end = narrow(&mut editor, std::slice::from_ref(&end));
    assert!(0 < row_end && row_end < 100, "first row ends at {row_end}");
    let line_end = narrow(&mut editor, &[end.clone(), end.clone()]);
    assert_eq!(line_end, text.len());
    // From the end: Home to the last row's start, then the line's.
    let all_end = press(Key::Named(Named::End), keyboard::Modifiers::COMMAND);
    let row_start = narrow(&mut editor, &[all_end.clone(), home.clone()]);
    assert!(
        row_start > 100 && row_start < text.len(),
        "last row starts at {row_start}"
    );
    assert_eq!(narrow(&mut editor, &[all_end, home.clone(), home]), 0);
}

#[test]
fn copy_and_cut_with_nothing_selected_take_the_line() {
    use iced::advanced::clipboard;

    let mut editor = Editor::new("one\ntwo\nthree".into());
    let second = Point::new(700.0, 16.0 + 24.0 + 10.0);
    run(&mut editor, |ui| {
        click(ui, second);
        ui.simulate([
            press(Key::Character("c".into()), keyboard::Modifiers::COMMAND),
            press(Key::Character("v".into()), keyboard::Modifiers::COMMAND),
        ]);
        let text = clipboard::Content::Text("two".into());
        ui.simulate([Event::Clipboard(clipboard::Event::Read(Ok(
            std::sync::Arc::new(text),
        )))]);
    });
    assert_eq!(
        editor.text(),
        "one\ntwo\ntwo\nthree",
        "pasted as a line above"
    );
    run(&mut editor, |ui| {
        click(ui, second);
        ui.simulate([press(
            Key::Character("x".into()),
            keyboard::Modifiers::COMMAND,
        )]);
    });
    assert_eq!(
        editor.text(),
        "one\ntwo\nthree",
        "the line and its ending cut"
    );
}
