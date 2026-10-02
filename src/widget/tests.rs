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
    editor.lines.borrow_mut().sized = true;
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
            other: false,
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
        other: false,
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
        other: false,
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
        other: false,
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
        other: false,
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
            other: false,
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
        other: false,
    };
    let _ = editor.update(Message(press));
    assert_eq!(editor.selection().head, 9, "between `b` and `o`");
    assert_eq!(hidden(&editor), before, "frozen while pressed");
    let _ = editor.update(Message(Input::Release));
    assert!(hidden(&editor).is_empty(), "revealed on release");
}

#[test]
fn a_far_jump_brings_the_caret_in_at_the_nearest_edge() {
    let text = "a line\n".repeat(300);
    let mut editor = Editor::new(text.clone());
    // As after a layout at the default size.
    editor.lines.borrow_mut().sized = true;
    let height = editor.lines.borrow().height;
    editor.select(text.len(), text.len());
    let y = editor.caret().expect("on screen").y;
    assert!(y > height * 0.8, "the end at the bottom: {y} of {height}");
    editor.select(0, 0);
    let y = editor.caret().expect("on screen").y;
    assert!(y < height * 0.2, "the start at the top: {y}");
}

#[test]
fn a_lazy_quote_line_draws_under_the_quoted_text() {
    let text = "> quoted text\nlazy line\n";
    let mut editor = Editor::new(text.into());
    editor.select(text.len(), text.len());
    editor.with_lines(|lines, source| {
        let (quoted, _, _) = lines.caret_in_line(source, 2, Affinity::After);
        let (lazy, top, _) = lines.caret_in_line(source, 14, Affinity::After);
        assert!(quoted > 4.0, "after the hidden `> `");
        assert!((lazy - quoted).abs() < 0.5, "{lazy} vs {quoted}");
        let line_top = lines.top_of(source, 1).expect("on screen");
        let (offset, _) = lines.hit(source, quoted + 1.0, line_top + top + 5.0);
        assert_eq!(offset, 14, "the lazy line's first character");
    });
}

#[test]
fn ticking_a_checkbox_far_from_the_caret_keeps_the_view() {
    use super::{Input, Message};
    let text = format!("{}- [ ] task\n", "line\n".repeat(200));
    let task_line = 200;
    let mut editor = Editor::new(text.clone());
    editor.select(0, 0);
    // Scroll the task into view, the caret staying at the top.
    let _ = editor.update(Message(Input::ScrollTo(1.0)));
    let anchor = editor.lines.borrow().anchor;
    let at = editor.with_lines(|lines, source| {
        let top = lines.top_of(source, task_line).expect("on screen");
        (0..120)
            .map(|x| iced::Point::new(x as f32, top + 12.0))
            .find(|p| lines.task_at(source, p.x, p.y).is_some())
            .expect("its checkbox")
    });
    let press = Input::Press {
        at,
        shift: false,
        clicks: 1,
        command: false,
        other: false,
    };
    let _ = editor.update(Message(press));
    // The hand moves a little before letting go.
    let _ = editor.update(Message(Input::Drag(iced::Point::new(at.x + 1.0, at.y))));
    let _ = editor.update(Message(Input::Release));
    assert!(editor.text().ends_with("- [x] task\n"));
    assert_eq!(editor.selection().head, 0, "the caret stays");
    assert_eq!(editor.lines.borrow().anchor, anchor, "and so does the view");
}

#[test]
fn a_grid_follows_links_only_on_cell_text_and_its_rule_maps_by_column() {
    use super::{Input, Message};
    let text = "> | a | [l](http://x.y) |\n> | - | - |\n> | b | c |\n\nend\n";
    let mut editor = Editor::new(text.into());
    editor.select(text.len(), text.len());
    let (x0, second, top0, top1) = editor.with_lines(|lines, source| {
        let grid = lines.grid(source, 0);
        let x0 = lines.grid_x(source, 0);
        let top0 = lines.top_of(source, 0).unwrap();
        let top1 = lines.top_of(source, 1).unwrap();
        (x0, grid.columns[1].0, top0, top1)
    });
    let press = |at: iced::Point| {
        Message(Input::Press {
            at,
            shift: false,
            clicks: 1,
            command: true,
            other: false,
        })
    };
    // Far right of the grid, on the link's row: nothing to follow.
    let task = editor.update(press(iced::Point::new(x0 + second + 150.0, top0 + 8.0)));
    assert!(outputs(task).iter().all(|m| m.link().is_none()));
    // On the link's text it follows.
    let task = editor.update(press(iced::Point::new(x0 + second + 10.0, top0 + 8.0)));
    assert_eq!(
        outputs(task)
            .iter()
            .filter_map(|m| m.link())
            .collect::<Vec<_>>(),
        ["http://x.y"]
    );
    // A click on the rule under the header goes to the header's cell in
    // that column.
    let offset =
        editor.with_lines(|lines, source| lines.table_hit(source, x0 + second + 2.0, top1 + 1.0));
    let offset = offset.expect("on the grid");
    let cell = editor.styled.tables()[0].rows[0].1[1].clone();
    assert!(
        cell.contains(&offset),
        "{offset} in the header's second cell {cell:?}"
    );
}

#[test]
fn a_release_that_reveals_markers_keeps_the_caret_in_view() {
    use super::{Input, Message};
    let url = format!("https://example.com/{}", "a".repeat(120));
    let text = format!("{}see [x]({url}) tail\nafter\n", "line\n".repeat(15));
    let mut editor = Editor::new(text.clone());
    editor.select(0, 0);
    let height = editor.lines.borrow().height;
    // Just after a link on line 15, the last line in view, its URL
    // hidden: on release the URL shows before the caret and wraps.
    let after = text.find(") tail").unwrap() + 1;
    let at = editor.with_lines(|lines, source| {
        let top = lines.top_of(source, 15).expect("on screen");
        let (x, _, _) = lines.caret_in_line(source, after, Affinity::After);
        iced::Point::new(x + 1.0, top + 5.0)
    });
    assert!(at.y + 24.0 <= height, "the line is in view");
    let press = Input::Press {
        at,
        shift: false,
        clicks: 1,
        command: false,
        other: false,
    };
    let _ = editor.update(Message(press));
    let _ = editor.update(Message(Input::Release));
    assert_eq!(editor.selection().head, after, "touching the link");
    let caret = editor.caret().expect("still on screen");
    assert!(
        caret.y + caret.height <= height + 0.5,
        "{caret:?} in {height}"
    );
}

#[test]
fn an_indented_lazy_line_lines_up_too() {
    let text = "- > quoted\n  lazy\n";
    let mut editor = Editor::new(text.into());
    editor.select(text.len(), text.len());
    editor.with_lines(|lines, source| {
        let (quoted, _, _) = lines.caret_in_line(source, 4, Affinity::After);
        let (lazy, _, _) = lines.caret_in_line(source, 13, Affinity::After);
        assert!((lazy - quoted).abs() < 0.5, "{lazy} vs {quoted}");
    });
}

/// A press and release at `at` with `clicks`, no modifier.
fn click_at(editor: &mut Editor, at: iced::Point, clicks: u8) {
    use super::{Input, Message};
    let press = Input::Press {
        at,
        shift: false,
        clicks,
        command: false,
        other: false,
    };
    let _ = editor.update(Message(press));
    let _ = editor.update(Message(Input::Release));
}

#[test]
fn a_double_click_selects_the_word_the_first_click_saw() {
    let text = "plain **bold** end\nother line\n";
    let mut editor = Editor::new(text.into());
    editor.select(text.len(), text.len());
    // On "b" of the bold word, its `**` hidden.
    let at = editor.with_lines(|lines, source| {
        let (x, _, _) = lines.caret_in_line(source, 8, Affinity::After);
        iced::Point::new(x + 2.0, 10.0)
    });
    click_at(&mut editor, at, 1);
    click_at(&mut editor, at, 2);
    let range = editor.selection().range();
    assert_eq!(&text[range], "bold");
}

#[test]
fn a_last_line_taller_than_the_view_scrolls_to_its_caret() {
    use super::{Input, Key, Message};
    let text = format!("short\n{}", "word ".repeat(600));
    let mut editor = Editor::new(text.clone());
    editor.select(text.len(), text.len());
    let _ = editor.update(Message(Input::Key(Key::Insert('x'))));
    let height = editor.lines.borrow().height;
    let caret = editor.caret().expect("on screen");
    assert!(
        caret.y + caret.height <= height + 0.5,
        "{caret:?} in {height}"
    );
}

#[test]
fn up_and_down_start_from_the_caret_even_far_out_of_view() {
    use super::{Input, Key, Message, Vertical};
    let text = "line\n".repeat(300);
    let mut editor = Editor::new(text);
    editor.select(0, 0);
    let _ = editor.update(Message(Input::Scroll(5000.0)));
    let _ = editor.update(Message(Input::Key(Key::Vertical(Vertical::Down, true))));
    assert_eq!(editor.selection().range(), 0..5, "line 1, not the view");
}

#[test]
fn releasing_after_scrolling_away_keeps_the_view() {
    use super::{Input, Message};
    let text = "line\n".repeat(300);
    let mut editor = Editor::new(text);
    let press = Input::Press {
        at: iced::Point::new(10.0, 10.0),
        shift: false,
        clicks: 1,
        command: false,
        other: false,
    };
    let _ = editor.update(Message(press.clone()));
    let _ = editor.update(Message(Input::Scroll(2000.0)));
    let anchor = editor.lines.borrow().anchor;
    let _ = editor.update(Message(Input::Release));
    assert_eq!(editor.lines.borrow().anchor, anchor);
    // Less than two screens away too.
    let _ = editor.update(Message(press));
    let _ = editor.update(Message(Input::Scroll(300.0)));
    let anchor = editor.lines.borrow().anchor;
    let _ = editor.update(Message(Input::Release));
    assert_eq!(editor.lines.borrow().anchor, anchor, "a short scroll");
}

#[test]
fn find_bar_inputs_that_move_nothing_keep_the_view() {
    use super::find::FindInput;
    use super::{Input, Message};
    let text = "a needle\n".to_owned() + &"line\n".repeat(300);
    let mut editor = Editor::new(text);
    editor.select(0, 0);
    let send = |editor: &mut Editor, input| {
        let _ = editor.update(Message(input));
    };
    send(&mut editor, Input::Find(FindInput::Open { replace: true }));
    send(&mut editor, Input::Find(FindInput::Query("needle".into())));
    send(&mut editor, Input::Scroll(3000.0));
    let anchor = editor.lines.borrow().anchor;
    send(
        &mut editor,
        Input::Find(FindInput::Replacement("pin".into())),
    );
    send(&mut editor, Input::Find(FindInput::Query("needlex".into())));
    send(&mut editor, Input::Find(FindInput::Close));
    assert_eq!(editor.lines.borrow().anchor, anchor);
}

#[test]
fn each_quick_click_on_a_checkbox_ticks_it() {
    let mut editor = Editor::new("- [ ] task\nnext".into());
    let end = editor.text().len();
    editor.select(end, end);
    let at = (0..120)
        .map(|x| iced::Point::new(x as f32, 12.0))
        .find(|p| editor.with_lines(|lines, source| lines.task_at(source, p.x, p.y).is_some()))
        .expect("a checkbox");
    click_at(&mut editor, at, 1);
    assert_eq!(editor.text(), "- [x] task\nnext");
    click_at(&mut editor, at, 2);
    assert_eq!(
        editor.text(),
        "- [ ] task\nnext",
        "the second click unticks"
    );
    assert_eq!(editor.selection().head, end);
}

#[test]
fn wrapped_rows_of_an_indented_lazy_line_line_up_too() {
    let text = format!("- > quoted\n  lazy {}\n", "word ".repeat(60));
    let mut editor = Editor::new(text.clone());
    editor.select(text.len(), text.len());
    editor.with_lines(|lines, source| {
        let (quoted, _, _) = lines.caret_in_line(source, 4, Affinity::After);
        let lazy = source.doc.line_range(1);
        let row = lines.row_bounds(source, lazy.end, Affinity::Before);
        assert!(row.start > lazy.start + 2, "a wrapped row");
        let (x, _, _) = lines.caret_in_line(source, row.start, Affinity::After);
        assert!((x - quoted).abs() < 0.5, "{x} vs {quoted}");
    });
}

#[test]
fn at_a_break_without_a_space_the_caret_goes_by_its_side() {
    let text =
        "a well-known state-of-the-art long-standing up-to-date self-contained hand-written\n";
    let editor = Editor::new(text.into());
    editor.lines.borrow_mut().width = 200.0;
    editor.with_lines(|lines, source| {
        // The start of the second row, which follows a hyphen.
        let row = lines.row_bounds(source, 0, Affinity::After);
        let next = row.end;
        assert!(
            text[..next].ends_with('-'),
            "a break after a hyphen at {next}"
        );
        let (x, top, _) = lines.caret_in_line(source, next, Affinity::After);
        assert!(x < 1.0 && top > 0.0, "the next row's start: {x}, {top}");
        let (_, top, _) = lines.caret_in_line(source, next, Affinity::Before);
        assert_eq!(top, 0.0, "Before: the first row's end");
        let row = lines.row_bounds(source, next, Affinity::After);
        assert_eq!(row.start, next, "End from there stays on that row");
    });
}

#[test]
fn a_click_on_a_bullet_dot_or_quote_bar_lands_at_the_text() {
    for (text, start) in [
        ("para\n- item text\n", 7),
        ("para\n> > quoted\n", 9),
        ("para\n>> nested\n", 8),
        ("para\n- [ ] task text\n", 11),
        ("para\n\n-\n", 6),
    ] {
        let mut editor = Editor::new(text.into());
        editor.select(0, 0);
        let line = text
            .lines()
            .position(|l| l.starts_with(['-', '>']))
            .unwrap();
        let top = editor.with_lines(|lines, source| lines.top_of(source, line).unwrap());
        let at = text.match_indices(['-', '>']).next().unwrap().0;
        let x =
            editor.with_lines(|lines, source| lines.caret_in_line(source, at, Affinity::After).0);
        click_at(&mut editor, iced::Point::new(x + 2.0, top + 10.0), 1);
        assert_eq!(editor.selection().head, start, "{text:?}");
    }
}

#[test]
fn a_selection_made_before_the_first_layout_is_revealed_by_it() {
    let text = "line\n".repeat(300);
    let mut editor = Editor::new(text.clone());
    editor.select(text.len(), text.len());
    assert_eq!(
        editor.lines.borrow().anchor,
        0,
        "not yet: the size is unknown"
    );
    assert!(editor.lines.borrow().pending_reveal.is_some());
}

#[test]
fn a_drag_from_the_margin_keeps_whole_list_lines() {
    use super::{Input, Message};
    let text = "para\n- one\n- two\n- three\n";
    let mut editor = Editor::new(text.into());
    editor.select(0, 0);
    let top = editor.with_lines(|lines, source| lines.top_of(source, 3).unwrap());
    let press = Input::Press {
        at: iced::Point::new(-4.0, 10.0),
        shift: false,
        clicks: 1,
        command: false,
        other: false,
    };
    let _ = editor.update(Message(press));
    let _ = editor.update(Message(Input::Drag(iced::Point::new(-4.0, top + 10.0))));
    let _ = editor.update(Message(Input::Release));
    assert_eq!(editor.selection().range(), 0..text.find("- three").unwrap());
}

#[test]
fn a_wrap_inside_two_spaces_keeps_the_caret_on_a_row_by_them() {
    let text = "The first sentence ends here.  Then another one follows and goes on for a while longer than the width allows it to.\n";
    let editor = Editor::new(text.into());
    editor.lines.borrow_mut().width = 210.0;
    editor.with_lines(|lines, source| {
        let (_, top, height) = lines.caret_in_line(source, 30, Affinity::After);
        assert!(top <= height, "row 0 or 1, not {top}");
        let row = lines.row_bounds(source, 30, Affinity::After);
        assert!(row.end < 100, "a row, not the whole line: {row:?}");
    });
}
