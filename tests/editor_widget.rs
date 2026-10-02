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

#[test]
fn enter_continues_a_list_and_tab_nests_the_new_item() {
    let mut editor = Editor::new("- a".into());
    run(&mut editor, |ui| {
        click(ui, Point::new(700.0, 20.0));
        ui.tap_key(Named::Enter);
        ui.typewrite("b");
        ui.tap_key(Named::Tab);
    });
    assert_eq!(editor.text(), "- a\n  - b");
    run(&mut editor, |ui| {
        click(ui, Point::new(700.0, 16.0 + 24.0 + 10.0));
        ui.simulate([press(Key::Named(Named::Tab), keyboard::Modifiers::SHIFT)]);
        ui.simulate([press(Key::Named(Named::Enter), keyboard::Modifiers::SHIFT)]);
    });
    assert_eq!(
        editor.text(),
        "- a\n- b\n  ",
        "Shift+Tab out, Shift+Enter under the text"
    );
}

#[test]
fn ctrl_b_e_and_k_format_the_selection() {
    let mut editor = Editor::new("word".into());
    let command = keyboard::Modifiers::COMMAND;
    run(&mut editor, |ui| {
        click(ui, Point::new(700.0, 20.0));
        ui.simulate([
            press(Key::Character("a".into()), command),
            press(Key::Character("b".into()), command),
        ]);
    });
    assert_eq!(editor.text(), "**word**");
    run(&mut editor, |ui| {
        click(ui, Point::new(700.0, 20.0));
        ui.simulate([press(Key::Character("e".into()), command)]);
        ui.typewrite("x");
    });
    assert_eq!(editor.text(), "**word**`x`", "a pair at the line's end");
    run(&mut editor, |ui| {
        click(ui, Point::new(700.0, 20.0));
        ui.simulate([press(Key::Character("k".into()), command)]);
        ui.typewrite("t");
    });
    assert_eq!(editor.text(), "**word**`x`[t]()");
}

#[test]
fn ctrl_f_finds_the_selection_and_f3_steps_through_matches() {
    let mut editor = Editor::new("cat, dog, cat, dog\n".into());
    let command = keyboard::Modifiers::COMMAND;
    let none = keyboard::Modifiers::default();
    let word = Point::new(16.0 + 8.0, 20.0);
    run(&mut editor, |ui| {
        click(ui, word);
        click(ui, word);
        ui.simulate([press(Key::Character("f".into()), command)]);
    });
    assert_eq!(selected(&editor), "cat", "the query is the selection");
    run(&mut editor, |ui| {
        ui.simulate([press(Key::Named(Named::F3), none)]);
    });
    assert_eq!(editor.selection().range(), 10..13, "the next cat");
    // Typing a new query in the bar selects its first match.
    run(&mut editor, |ui| {
        ui.click("cat").expect("the query field");
        ui.simulate([press(Key::Character("a".into()), command)]);
        ui.typewrite("dog");
    });
    assert_eq!(editor.selection().range(), 5..8, "the first dog");
    run(&mut editor, |ui| {
        ui.simulate([press(Key::Named(Named::Escape), none)]);
        ui.simulate([press(Key::Named(Named::F3), none)]);
    });
    assert_eq!(
        editor.selection().range(),
        5..8,
        "closed: F3 no longer moves on"
    );
}

#[test]
fn ctrl_shift_e_shows_the_markdown_as_written() {
    let mut editor = Editor::new("# Title\n\nbody".into());
    let first_glyph = Point::new(16.0 + 1.0, 20.0);
    // The caret on the body line: the heading's `# ` is hidden, so the
    // first glyph is the T.
    run(&mut editor, |ui| {
        click(ui, Point::new(700.0, 16.0 + 34.0 + 24.0 + 5.0))
    });
    run(&mut editor, |ui| {
        click(ui, Point::new(700.0, 16.0 + 34.0 + 24.0 + 5.0));
        let source = keyboard::Modifiers::COMMAND | keyboard::Modifiers::SHIFT;
        ui.simulate([press(Key::Character("e".into()), source)]);
    });
    assert_eq!(editor.mode(), livemark::widget::Mode::Source);
    run(&mut editor, |ui| click(ui, first_glyph));
    assert_eq!(editor.selection().head, 0, "the `#` shows in source mode");
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
fn alt_arrows_move_and_copy_lines_and_ctrl_shift_k_deletes_them() {
    let mut editor = Editor::new("one\ntwo\nthree".into());
    let alt = keyboard::Modifiers::ALT;
    // A click on the first line, right of its text.
    run(&mut editor, |ui| {
        click(ui, Point::new(700.0, 20.0));
        ui.simulate([press(Key::Named(Named::ArrowDown), alt)]);
    });
    assert_eq!(editor.text(), "two\none\nthree");
    assert_eq!(caret_line(&editor), 1, "the caret goes with its line");
    run(&mut editor, |ui| {
        click(ui, Point::new(700.0, 20.0));
        ui.simulate([press(
            Key::Named(Named::ArrowDown),
            alt | keyboard::Modifiers::SHIFT,
        )]);
        ui.simulate([press(
            Key::Character("k".into()),
            keyboard::Modifiers::COMMAND | keyboard::Modifiers::SHIFT,
        )]);
        ui.simulate([press(
            Key::Named(Named::Enter),
            keyboard::Modifiers::COMMAND,
        )]);
        ui.typewrite("new");
    });
    // Copied down, the lower copy deleted (the caret drops to `one`), a
    // blank line under it.
    assert_eq!(editor.text(), "two\none\nnew\nthree");
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
fn the_editor_keeps_focus_when_the_find_bar_opens() {
    use iced::advanced::renderer::Headless as _;
    use iced_runtime::user_interface::{Cache, UserInterface};
    // A widget tree kept from frame to frame, as in an app (the simulator
    // builds a new one every round).
    let backend = std::env::var("ICED_TEST_BACKEND").unwrap_or_else(|_| "tiny-skia".into());
    let renderer = iced::Renderer::new(
        iced::advanced::renderer::Settings::default(),
        Some(&backend),
    );
    let mut renderer = iced::futures::executor::block_on(renderer).expect("a headless renderer");
    let mut editor = Editor::new("one two\n".into());
    let mut cache = Cache::default();
    let mut frame = |editor: &mut Editor, cache: Cache, at: Point, events: &[Event]| {
        let mut messages = iced::advanced::shell::Bus::<Message>::new();
        let mut ui = UserInterface::build(
            editor.view(),
            iced::Size::new(800.0, 600.0),
            cache,
            &mut renderer,
        );
        let _ = ui.update(
            &iced::window::Headless,
            &iced::advanced::shell::Waker::noop(),
            events,
            mouse::Cursor::Available(at),
            &mut renderer,
            &mut messages,
        );
        let cache = ui.into_cache();
        for message in messages {
            let _ = editor.update(message);
        }
        cache
    };
    let at = Point::new(700.0, 20.0);
    cache = frame(
        &mut editor,
        cache,
        at,
        &[
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
        ],
    );
    // Ctrl+F opens the bar (its field's focus is a task, not run here).
    cache = frame(
        &mut editor,
        cache,
        at,
        &[press(
            Key::Character("f".into()),
            keyboard::Modifiers::COMMAND,
        )],
    );
    let typed = Event::Keyboard(keyboard::Event::KeyPressed {
        key: Key::Character("!".into()),
        modified_key: Key::Character("!".into()),
        physical_key: keyboard::key::Physical::Unidentified(
            keyboard::key::NativeCode::Unidentified,
        ),
        location: keyboard::Location::Standard,
        modifiers: keyboard::Modifiers::empty(),
        text: Some("!".into()),
        repeat: false,
    });
    let _ = frame(&mut editor, cache, at, &[typed]);
    assert_eq!(editor.text(), "one two!\n", "the editor still had focus");
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
    // The icon buttons, 30 wide and 2 apart in two groups (2 padding each
    // side, 8 between): button `i` is centred at this x, 17 down.
    let icon_x = |i: usize| 2.0 + (i / 4) as f32 * 138.0 + (i % 4) as f32 * 32.0 + 15.0;
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
        press_at(&mut editor, &|ui| click(ui, Point::new(icon_x(i), 17.0)));
        assert_eq!(editor.text(), after, "button {i}");
    }
    // The link button: `[text]()` around the selection.
    let mut editor = Editor::new("word".into());
    editor.select(0, 4);
    press_at(&mut editor, &|ui| click(ui, Point::new(icon_x(3), 17.0)));
    assert_eq!(editor.text(), "[word]()");
    // The mode switch, by its names.
    let mut editor = Editor::new("text".into());
    for (label, mode) in [
        ("Markdown", livemark::widget::Mode::Source),
        ("Side by side", livemark::widget::Mode::Split),
        ("Live preview", livemark::widget::Mode::Live),
    ] {
        press_at(&mut editor, &|ui| {
            ui.click(label)
                .unwrap_or_else(|_| panic!("a {label} button"));
        });
        assert_eq!(editor.mode(), mode, "{label}");
    }
}

#[test]
fn ctrl_shift_e_cycles_live_source_and_split() {
    use livemark::widget::Mode;
    let mut editor = Editor::new("# Title\n\nSome text.\n".into());
    // Ctrl+Shift+E cycles Live, Source, Split, Live.
    let mut modes = Vec::new();
    for _ in 0..3 {
        run(&mut editor, |ui| {
            click(ui, Point::new(30.0, 30.0));
            ui.simulate([press(
                Key::Character("e".into()),
                keyboard::Modifiers::COMMAND | keyboard::Modifiers::SHIFT,
            )]);
        });
        modes.push(editor.mode());
    }
    assert_eq!(modes, [Mode::Source, Mode::Split, Mode::Live]);
    assert_eq!(editor.mode(), Mode::Live);
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
