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
        };
        let _ = editor.update(Message(press));
        let _ = editor.update(Message(Input::Release));
    };
    click(&mut editor, at);
    assert_eq!(editor.text(), "- [x] task\nnext");
    assert_eq!(editor.selection().head, end, "the caret stays");
    click(&mut editor, at);
    assert_eq!(editor.text(), "- [ ] task\nnext");
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
