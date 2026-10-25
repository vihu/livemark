//! The editor widget driven through iced's headless simulator: clicks place
//! the caret, typing and input method commits edit there, the scroll bar,
//! the wheel, the toolbar and pastes (keys in `editor_keys.rs`).
use iced::keyboard::{self, Key, key::Named};
use iced::{Event, Point, mouse};
use livemark::widget::{Editor, Message};
use sim::{SIZE, caret_line, click, press, run, selected};

mod sim;

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
fn pressing_the_scroll_bar_track_jumps_there() {
    let text: String = (0..500).map(|i| format!("line {i}\n")).collect();
    let mut editor = Editor::new(text);
    // The track runs down the right edge, inside the padding.
    run(&mut editor, |ui| {
        click(ui, Point::new(SIZE.0 - 7.0, SIZE.1 - 6.0))
    });
    run(&mut editor, |ui| click(ui, Point::new(100.0, 20.0)));
    assert!(caret_line(&editor) > 450, "line {}", caret_line(&editor));
    run(&mut editor, |ui| click(ui, Point::new(SIZE.0 - 7.0, 6.0)));
    run(&mut editor, |ui| click(ui, Point::new(100.0, 20.0)));
    assert_eq!(caret_line(&editor), 0);
}

#[test]
fn a_failed_paste_does_not_take_a_later_read_meant_for_another_widget() {
    use iced::advanced::clipboard;
    let mut editor = Editor::new("one\ntwo\n".into());
    run(&mut editor, |ui| {
        click(ui, Point::new(700.0, 20.0));
        // Ctrl+V with an image, or nothing, on the clipboard: the read fails.
        ui.simulate([press(
            Key::Character("v".into()),
            keyboard::Modifiers::COMMAND,
        )]);
        ui.simulate([Event::Clipboard(clipboard::Event::Read(Err(
            clipboard::Error::ContentNotAvailable,
        )))]);
        // Later another widget reads the clipboard.
        click(ui, Point::new(700.0, 590.0));
        let text = clipboard::Content::Text("PASTED".into());
        ui.simulate([Event::Clipboard(clipboard::Event::Read(Ok(
            std::sync::Arc::new(text),
        )))]);
    });
    assert_eq!(editor.text(), "one\ntwo\n");
}

#[test]
fn ctrl_with_the_wheel_zooms_and_without_it_scrolls() {
    let mut editor = Editor::new("line\n".repeat(200));
    let wheel = |y: f32| {
        Event::Mouse(mouse::Event::WheelScrolled {
            delta: mouse::ScrollDelta::Lines { x: 0.0, y },
        })
    };
    run(&mut editor, |ui| {
        ui.point_at(Point::new(30.0, 30.0));
        ui.simulate([
            Event::Keyboard(keyboard::Event::ModifiersChanged(
                keyboard::Modifiers::COMMAND,
            )),
            wheel(1.0),
            wheel(1.0),
        ]);
    });
    assert!((editor.zoom() - 1.2).abs() < 1e-6, "{}", editor.zoom());
    run(&mut editor, |ui| {
        ui.point_at(Point::new(30.0, 30.0));
        ui.simulate([
            Event::Keyboard(keyboard::Event::ModifiersChanged(
                keyboard::Modifiers::COMMAND,
            )),
            wheel(-1.0),
            Event::Keyboard(keyboard::Event::ModifiersChanged(
                keyboard::Modifiers::empty(),
            )),
            wheel(-3.0),
        ]);
    });
    assert!(
        (editor.zoom() - 1.1).abs() < 1e-6,
        "the plain wheel scrolls"
    );
}

#[test]
fn toolbar_buttons_run_their_keys_commands() {
    // A press on the toolbar, then the messages applied.
    let press_at = |editor: &mut Editor, find: &dyn Fn(&mut iced_test::Simulator<'_, Message>)| {
        let messages: Vec<_> = {
            let view = iced::widget::column![editor.toolbar(), editor.view()];
            let mut ui = iced_test::Simulator::with_size(iced::Settings::default(), SIZE, view);
            find(&mut ui);
            ui.into_messages().collect()
        };
        for message in messages {
            let _ = editor.update(message);
        }
    };
    // The icon buttons, 30 wide and 2 apart in two groups with a hairline
    // between (6, 1, 6): button `i` is centred at this x, 16 down in the
    // bar the mode switch makes 32 tall.
    let icon_x = |i: usize| (i / 4) as f32 * 139.0 + (i % 4) as f32 * 32.0 + 15.0;
    for (i, before, after) in [
        (0, "word", "**word**"),
        (1, "word", "*word*"),
        (2, "word", "`word`"),
        (4, "Title", "# Title"),
        (5, "milk", "- milk"),
        (6, "milk", "- [ ] milk"),
        (7, "a", "> a"),
    ] {
        let mut editor = Editor::new(before.into());
        editor.select(0, before.len());
        press_at(&mut editor, &|ui| click(ui, Point::new(icon_x(i), 16.0)));
        assert_eq!(editor.text(), after, "button {i}");
    }
    // The link button: `[text]()` around the selection.
    let mut editor = Editor::new("word".into());
    editor.select(0, 4);
    press_at(&mut editor, &|ui| click(ui, Point::new(icon_x(3), 16.0)));
    assert_eq!(editor.text(), "[word]()");
    // The mode switch at the right edge: three icons, 30 wide and 2
    // apart, in 2 of padding; their names are in their tooltips.
    let mode_x = |j: usize| SIZE.0 - 98.0 + 2.0 + j as f32 * 32.0 + 15.0;
    let mut editor = Editor::new("text".into());
    for (j, mode) in [
        (1, livemark::widget::Mode::Source),
        (2, livemark::widget::Mode::Split),
        (0, livemark::widget::Mode::Live),
    ] {
        press_at(&mut editor, &|ui| click(ui, Point::new(mode_x(j), 16.0)));
        assert_eq!(editor.mode(), mode, "mode {j}");
    }
}

#[test]
fn a_paste_with_no_text_hands_the_host_the_picture() {
    use iced::advanced::clipboard;
    let editor = Editor::new("one\n".into());
    let messages: Vec<Message> = {
        let mut ui =
            iced_test::Simulator::with_size(iced::Settings::default(), SIZE, editor.view());
        click(&mut ui, Point::new(30.0, 20.0));
        ui.simulate([press(
            Key::Character("v".into()),
            keyboard::Modifiers::COMMAND,
        )]);
        // No text: the editor asks for a picture, and gets one.
        ui.simulate([Event::Clipboard(clipboard::Event::Read(Err(
            clipboard::Error::ContentNotAvailable,
        )))]);
        let image = clipboard::Image {
            rgba: vec![0u8; 2 * 2 * 4].into(),
            size: iced::Size::new(2, 2),
        };
        ui.simulate([Event::Clipboard(clipboard::Event::Read(Ok(
            std::sync::Arc::new(clipboard::Content::Image(image)),
        )))]);
        ui.into_messages().collect()
    };
    assert!(messages.iter().any(|m| m.pasted_image().is_some()));
}
