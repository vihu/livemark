//! Presses in the text: checkboxes, bullets and quote bars, double
//! clicks, drags from the margin, and markers kept as drawn until the
//! release (REFERENCE-001 sections 2, 8 and 14).
use super::Editor;
use crate::layout::Affinity;

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
fn ticking_a_checkbox_far_from_the_caret_keeps_the_view() {
    use super::{Input, Message};
    let text = format!("{}- [ ] task\n", "line\n".repeat(200));
    let task_line = 200;
    let mut editor = Editor::new(text.clone());
    editor.select(0, 0);
    // Scroll the task into view, the caret staying at the top.
    let _ = editor.update(Message(Input::ScrollTo(super::Pane::Text, 1.0)));
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
    let _ = editor.update(Message(Input::Scroll(super::Pane::Text, 2000.0)));
    let anchor = editor.lines.borrow().anchor;
    let _ = editor.update(Message(Input::Release));
    assert_eq!(editor.lines.borrow().anchor, anchor);
    // Less than two screens away too.
    let _ = editor.update(Message(press));
    let _ = editor.update(Message(Input::Scroll(super::Pane::Text, 300.0)));
    let anchor = editor.lines.borrow().anchor;
    let _ = editor.update(Message(Input::Release));
    assert_eq!(editor.lines.borrow().anchor, anchor, "a short scroll");
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
fn a_click_on_a_bullet_dot_or_quote_bar_lands_at_the_text() {
    for (text, start) in [
        ("para\n- item text\n", 7),
        ("para\n> > quoted\n", 9),
        ("para\n>> nested\n", 8),
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
