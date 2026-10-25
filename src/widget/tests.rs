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
    {
        // As a layout leaves it.
        let mut lines = editor.lines.borrow_mut();
        lines.outer = iced::Size::new(800.0, 600.0);
        lines.fit();
        lines.sized = true;
    }
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
pub(super) fn outputs(task: iced::Task<super::Message>) -> Vec<super::Message> {
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
    let _ = editor.update(Message(Input::Scroll(super::Pane::Text, 5000.0)));
    let _ = editor.update(Message(Input::Key(Key::Vertical(Vertical::Down, true))));
    assert_eq!(editor.selection().range(), 0..5, "line 1, not the view");
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
    send(&mut editor, Input::Scroll(super::Pane::Text, 3000.0));
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

#[test]
fn a_line_width_caps_the_text_at_its_code_columns_and_centres_it() {
    let line = |columns: usize| format!("```\n{}\n```\n", "x".repeat(columns));
    let rows = |text: String, zoom: f32| {
        let mut editor = Editor::new(text);
        editor.set_zoom(zoom);
        editor.set_line_width(Some(120));
        let mut lines = editor.lines.borrow_mut();
        lines.outer = iced::Size::new(3000.0, 600.0);
        lines.fit();
        let area = super::surface::text_area(
            iced::Rectangle::new(iced::Point::ORIGIN, lines.outer),
            zoom,
            Some(120),
        );
        assert!((area.center_x() - 1500.0).abs() < 0.5, "centred: {area:?}");
        drop(lines);
        editor.with_lines(|lines, source| lines.shaped(source, 1).buffer.layout_runs().count())
    };
    for zoom in [1.0, 1.7] {
        assert_eq!(rows(line(120), zoom), 1, "120 columns fit at {zoom}");
        assert_eq!(rows(line(121), zoom), 2, "121 wrap at {zoom}");
    }
    // Narrower than the cap, the text fills the width as before.
    let bounds = iced::Rectangle::new(iced::Point::ORIGIN, iced::Size::new(600.0, 400.0));
    assert_eq!(
        super::surface::text_area(bounds, 1.0, Some(120)),
        super::surface::text_area(bounds, 1.0, None)
    );
}
