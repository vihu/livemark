//! Geometry inside the widget; the simulator tests are in
//! `tests/editor_widget.rs`.
use super::Editor;
use crate::layout::Affinity;

#[test]
fn a_hanging_row_maps_clicks_and_the_caret_after_the_marker() {
    let editor = Editor::new(format!("- {}\n", "word ".repeat(30)));
    editor.lines.borrow_mut().width = 200.0;
    editor.with_lines(|lines, source| {
        let shaped = lines.shaped(source, 0);
        assert!(
            shaped.hang > 4.0,
            "rows after the first hang ({})",
            shaped.hang
        );
        // Just right of where the second row's text starts.
        let (offset, _) = lines.hit(source, shaped.hang + 1.0, shaped.first_row + 1.0);
        let row = lines.row_bounds(source, offset, Affinity::After);
        assert_eq!(offset, row.start, "the start of the second row's text");
        let (x, top, _) = lines.caret_in_line(source, offset, Affinity::After);
        assert!(
            (x - shaped.hang).abs() < 1.0,
            "caret at {x}, hang {}",
            shaped.hang
        );
        assert_eq!(top, shaped.first_row);
    });
}

#[test]
fn switching_modes_keeps_the_caret_row_where_it_was() {
    use super::Mode;
    let text = "# A heading\n\nSome **bold** text in a paragraph.\n\n".repeat(60);
    let mut editor = Editor::new(text);
    let middle = editor.text().len() / 2;
    editor.select(middle, middle);
    let y = |editor: &Editor| editor.caret().expect("on screen").y;
    let before = y(&editor);
    editor.set_mode(Mode::Source);
    assert!(
        (y(&editor) - before).abs() < 1.0,
        "{} vs {before}",
        y(&editor)
    );
    editor.set_mode(Mode::Live);
    assert!((y(&editor) - before).abs() < 1.0);
}

#[test]
fn a_click_on_a_checkbox_toggles_the_task_and_keeps_the_caret() {
    use super::{Input, Message};
    use iced::Point;
    let mut editor = Editor::new("- [ ] task\nnext".into());
    let end = editor.text().len();
    editor.select(end, end);
    // Somewhere along the first row, the checkbox.
    let at = (0..120)
        .map(|x| Point::new(x as f32, 12.0))
        .find(|p| editor.with_lines(|lines, source| lines.task_at(source, p.x, p.y).is_some()))
        .expect("a checkbox on the first row");
    let click = |editor: &mut Editor, at: Point| {
        let press = Input::Press {
            at,
            shift: false,
            clicks: 1,
            command: false,
            alt: false,
        };
        let _ = editor.update(Message(press));
        let _ = editor.update(Message(Input::Release));
    };
    click(&mut editor, at);
    assert_eq!(editor.text(), "- [x] task\nnext");
    assert_eq!(editor.selection().head, end, "the caret stays");
    click(&mut editor, at);
    assert_eq!(editor.text(), "- [ ] task\nnext");
    // With Ctrl or Cmd it is an ordinary click (REFERENCE-001 section 8).
    let press = Input::Press {
        at,
        shift: false,
        clicks: 1,
        command: true,
        alt: false,
    };
    let _ = editor.update(Message(press));
    let _ = editor.update(Message(Input::Release));
    assert_eq!(editor.text(), "- [ ] task\nnext", "not toggled");
    // On the task's text, a click places the caret.
    click(&mut editor, Point::new(at.x + 40.0, at.y));
    assert_eq!(editor.text(), "- [ ] task\nnext");
    assert!(editor.selection().head < end);
}

#[test]
fn nothing_is_revealed_before_the_caret_is_first_placed() {
    let mut editor = Editor::new("# Title\n\n- item\n".into());
    let hidden = |editor: &Editor| editor.with_lines(|_, source| source.hidden.to_vec());
    let concealed = |editor: &Editor| editor.with_lines(|_, source| source.concealed.len());
    assert_eq!(
        hidden(&editor),
        std::slice::from_ref(&(0..2)),
        "the heading's `# ` with the caret at 0"
    );
    assert_eq!(concealed(&editor), 1);
    // The first click lands where the clean heading was drawn.
    let at = iced::Point::new(40.0, 10.0);
    let (offset, _) = editor.with_lines(|lines, source| lines.hit(source, at.x, at.y));
    assert!(offset > 2, "on the heading's text");
    let press = super::Input::Press {
        at,
        shift: false,
        clicks: 1,
        command: false,
        alt: false,
    };
    let _ = editor.update(super::Message(press));
    assert_eq!(
        hidden(&editor),
        std::slice::from_ref(&(0..2)),
        "still clean while pressed"
    );
    let _ = editor.update(super::Message(super::Input::Release));
    assert_eq!(editor.selection().head, offset);
    assert!(hidden(&editor).is_empty(), "placed at the heading");
}

#[test]
fn a_click_on_a_grid_cell_puts_the_caret_in_its_source() {
    use super::{Input, Message};
    let text = "x\n\n| a | bb |\n| - | - |\n| c | **dd** |\n";
    let mut editor = Editor::new(text.into());
    editor.select(text.len(), text.len());
    // The left edge of row 2's second column, a little in.
    let at = editor.with_lines(|lines, source| {
        let grid = lines.grid(source, 0);
        let top = lines.top_of(source, 4).expect("on screen");
        let shaped = lines.shaped(source, 4);
        let start = super::marks::start_x(&shaped, source.doc.line_range(4).start);
        iced::Point::new(start + grid.columns[1].0 + 9.0, top + 5.0)
    });
    let _ = editor.update(Message(Input::Press {
        at,
        shift: false,
        clicks: 1,
        command: false,
        alt: false,
    }));
    let _ = editor.update(Message(Input::Release));
    let head = editor.selection().head;
    assert_eq!(&text[head..head + 2], "dd", "after the hidden `**`");
    // Text typed above moves the table; its grid is shared by text, so a
    // click must still land in the moved source.
    editor.select(0, 0);
    let _ = editor.update(Message(Input::Key(super::Key::Insert('z'))));
    let end = editor.text().len();
    editor.select(end, end);
    let _ = editor.update(Message(Input::Press {
        at,
        shift: false,
        clicks: 1,
        command: false,
        alt: false,
    }));
    let _ = editor.update(Message(Input::Release));
    let head = editor.selection().head;
    assert_eq!(&editor.text()[head..head + 2], "dd", "after an edit above");
}

#[test]
fn ctrl_click_and_alt_enter_hand_a_link_to_the_host() {
    use super::{Input, Key, Message};
    let text = "see [the docs](https://x.y/d) and www.a.b now\n";
    let mut editor = Editor::new(text.into());
    editor.select(text.len(), text.len());
    let x_of = |editor: &Editor, offset: usize| {
        editor.with_lines(|lines, source| lines.caret_in_line(source, offset, Affinity::After).0)
    };
    let press = |at: iced::Point, command: bool| {
        Message(Input::Press {
            at,
            shift: false,
            clicks: 1,
            command,
            alt: false,
        })
    };
    // Ctrl+click on the link text (its markers hidden, the caret elsewhere).
    let at = iced::Point::new(x_of(&editor, 8) + 1.0, 10.0);
    let links = |task| -> Vec<String> {
        outputs(task)
            .iter()
            .filter_map(|m| m.link().map(str::to_owned))
            .collect()
    };
    let task = editor.update(press(at, true));
    assert_eq!(links(task), ["https://x.y/d"]);
    assert_eq!(editor.selection().head, text.len(), "the caret stays");
    assert_eq!(editor.styled.link_at(8), Some("https://x.y/d"));
    assert_eq!(editor.styled.link_at(36), Some("http://www.a.b"));
    assert_eq!(editor.styled.link_at(1), None);
    // Beside a line that ends with a link, which hits the link's end:
    // nothing to follow there.
    let mut ending = Editor::new("see [docs](https://x.y/d)\n".into());
    ending.select(27, 27);
    assert!(x_of(&ending, 25) < 500.0);
    assert!(links(ending.update(press(iced::Point::new(500.0, 10.0), true))).is_empty());
    let on = iced::Point::new(x_of(&ending, 6), 10.0);
    assert_eq!(links(ending.update(press(on, true))), ["https://x.y/d"]);
    // A plain click places the caret, as before.
    let _ = editor.update(press(at, false));
    let _ = editor.update(Message(Input::Release));
    assert!(editor.selection().head < 15);
    let task = editor.update(Message(Input::Key(Key::Follow)));
    assert_eq!(links(task), ["https://x.y/d"], "Alt+Enter at the caret");
}

/// The messages a task from `Editor::update` resolves to.
fn outputs(task: iced::Task<super::Message>) -> Vec<super::Message> {
    use iced::futures::StreamExt;
    let Some(stream) = iced_runtime::task::into_stream(task) else {
        return Vec::new();
    };
    iced::futures::executor::block_on(stream.collect::<Vec<_>>())
        .into_iter()
        .filter_map(|action| match action {
            iced_runtime::Action::Output(message) => Some(message),
            _ => None,
        })
        .collect()
}

#[test]
fn a_grid_starts_where_its_rows_source_starts() {
    let text = "- | a |\n  | - |\n  | b |\n\nend";
    let mut editor = Editor::new(text.into());
    editor.select(text.len(), text.len());
    editor.with_lines(|lines, source| {
        let shaped = lines.shaped(source, 0);
        let x = super::marks::start_x(&shaped, 2);
        assert!(x > 5.0, "after the bullet: {x}");
    });
}

#[test]
fn a_cached_grid_follows_a_link_definition_added_elsewhere() {
    use super::{Input, Key, Message};
    let mut editor = Editor::new("| [a] |\n| - |\n\n".into());
    let cell = |editor: &Editor| {
        editor.with_lines(|lines, source| lines.grid(source, 0).cells[1 - 1][0].line.text.clone())
    };
    let end = editor.text().len();
    editor.select(end, end);
    assert_eq!(cell(&editor), "[a]", "no definition: text");
    for c in "[a]: http://x.y".chars() {
        let _ = editor.update(Message(Input::Key(Key::Insert(c))));
    }
    assert_eq!(cell(&editor), "a", "a reference link now");
}

#[test]
fn enter_in_the_find_field_with_shift_held_steps_back() {
    use super::find::FindInput;
    use super::{Input, Message};
    let mut editor = Editor::new("a x a x a".into());
    editor.select(4, 5);
    let send = |editor: &mut Editor, input| {
        let _ = editor.update(Message(input));
    };
    send(&mut editor, Input::Find(FindInput::Open { replace: false }));
    send(&mut editor, Input::Find(FindInput::Query("a".into())));
    // Enter submits a step forward; with Shift held it goes back.
    send(&mut editor, Input::Shift(true));
    send(&mut editor, Input::Find(FindInput::Step { forward: true }));
    assert_eq!(editor.selection().range(), 0..1);
    send(&mut editor, Input::Shift(false));
    send(&mut editor, Input::Find(FindInput::Step { forward: true }));
    assert_eq!(editor.selection().range(), 4..5);
}

#[test]
fn a_press_keeps_markers_as_drawn_until_the_release() {
    use super::{Input, Message};
    let text = "plain **bold** end\n";
    let mut editor = Editor::new(text.into());
    editor.select(text.len(), text.len());
    let hidden = |editor: &Editor| editor.with_lines(|_, source| source.hidden.to_vec());
    let before = hidden(&editor);
    assert_eq!(before.len(), 2, "the bold markers");
    // A press on the bold text: the caret goes there, the text stays put.
    let x = editor.with_lines(|lines, source| lines.caret_in_line(source, 9, Affinity::After).0);
    let at = iced::Point::new(x, 10.0);
    let press = Input::Press {
        at,
        shift: false,
        clicks: 1,
        command: false,
        alt: false,
    };
    let _ = editor.update(Message(press));
    assert_eq!(editor.selection().head, 9, "between `b` and `o`");
    assert_eq!(hidden(&editor), before, "frozen while pressed");
    let _ = editor.update(Message(Input::Release));
    assert!(hidden(&editor).is_empty(), "revealed on release");
}
