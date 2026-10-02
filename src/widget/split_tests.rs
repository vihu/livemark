//! Side by side (PLAN-003): the rendered pane hides every marker and
//! follows edits, the panes scroll together both ways, and a click in the
//! rendered pane puts the caret there.
use iced::Point;

use super::{Editor, Input, Message, Mode, Pane};

/// An editor side by side, both panes laid out as a 1000 by 600 row
/// leaves them.
fn side_by_side(text: String) -> Editor {
    let mut editor = Editor::new(text);
    editor.set_mode(Mode::Split);
    for lines in [&editor.lines, &editor.preview.lines] {
        let mut lines = lines.borrow_mut();
        lines.outer = iced::Size::new(500.0, 600.0);
        lines.fit();
        lines.sized = true;
    }
    editor
}

/// A pane's top line and how far into it, as a share of its height.
fn top(editor: &Editor, pane: Pane) -> (usize, f32) {
    editor.with_pane(pane, |lines, source| {
        let height = lines.shaped(source, lines.anchor).height;
        (lines.anchor, lines.offset / height)
    })
}

/// Line `index` as `pane` shows it.
fn shown(editor: &Editor, pane: Pane, index: usize) -> String {
    editor.with_pane(pane, |lines, source| lines.shaped(source, index).line.text)
}

fn send(editor: &mut Editor, input: Input) {
    let _ = editor.update(Message(input));
}

#[test]
fn the_rendered_pane_hides_every_marker_and_follows_edits() {
    let mut editor = side_by_side("# Title\n**bold** and `code`\n".into());
    editor.select(3, 3);
    assert_eq!(shown(&editor, Pane::Text, 0), "# Title");
    assert_eq!(shown(&editor, Pane::Preview, 0), "Title", "caret or not");
    assert_eq!(shown(&editor, Pane::Preview, 1), "bold and code");
    let end = editor.text().len();
    editor.select(end, end);
    send(&mut editor, Input::Commit("*more*".into()));
    assert_eq!(shown(&editor, Pane::Preview, 2), "more");
}

#[test]
fn the_panes_scroll_together_by_source_line_both_ways() {
    let text = "# Heading\n\nSome **bold** text.\n\n".repeat(80);
    let mut editor = side_by_side(text);
    let close = |a: (usize, f32), b: (usize, f32)| a.0 == b.0 && (a.1 - b.1).abs() < 0.01;
    send(&mut editor, Input::Scroll(Pane::Text, 1010.0));
    let text_top = top(&editor, Pane::Text);
    assert!(text_top.0 > 10 && text_top.1 > 0.0, "{text_top:?}");
    let preview_top = top(&editor, Pane::Preview);
    assert!(close(text_top, preview_top), "{text_top:?} {preview_top:?}");
    // The rendered pane's headings are taller: scrolled there, the text
    // follows it to the same line.
    send(&mut editor, Input::Scroll(Pane::Preview, 1010.0));
    let preview_top = top(&editor, Pane::Preview);
    let text_top = top(&editor, Pane::Text);
    assert!(close(text_top, preview_top), "{text_top:?} {preview_top:?}");
    // What moves neither leaves both where they are.
    send(&mut editor, Input::Shift(true));
    assert_eq!(top(&editor, Pane::Preview), preview_top);
    assert_eq!(top(&editor, Pane::Text), text_top);
    // A caret moved in the text leads again.
    let middle = editor.text().len() / 2;
    editor.select(middle, middle);
    // As the next frame's draw does.
    editor.follow();
    assert!(close(top(&editor, Pane::Text), top(&editor, Pane::Preview)));
    // At the text's end, the rendered pane at its own end.
    send(&mut editor, Input::ScrollTo(Pane::Text, 1.0));
    let end = editor.with_pane(Pane::Preview, |lines, source| {
        ((lines.anchor, lines.offset), lines.end(source))
    });
    assert_eq!(end.0, end.1);
}

#[test]
fn a_click_in_the_rendered_pane_puts_the_caret_there_in_the_markdown() {
    let text = "# Title\n\nfirst **bold** words\n- [ ] task\n";
    let mut editor = side_by_side(text.into());
    let point = |editor: &Editor, offset: usize| {
        editor.with_pane(Pane::Preview, |lines, source| {
            let index = source.doc.line_at(offset);
            let top = lines.top_of(source, index).expect("on screen");
            let (x, row, _) = lines.caret_in_line(source, offset, crate::layout::Affinity::After);
            Point::new(x + 1.0, top + row + 5.0)
        })
    };
    let bold = text.find("bold").unwrap();
    let at = point(&editor, bold);
    send(&mut editor, Input::PreviewPress { at, command: false });
    assert_eq!(editor.selection().head, bold, "past the hidden `**`");
    assert!(editor.placed);
    // A checkbox there ticks the task and leaves the caret.
    let tick = (0..120)
        .map(|x| Point::new(x as f32, at.y + 24.0))
        .find(|p| {
            editor.with_pane(Pane::Preview, |lines, source| {
                lines.task_at(source, p.x, p.y).is_some()
            })
        })
        .expect("a checkbox on the task's row");
    send(
        &mut editor,
        Input::PreviewPress {
            at: tick,
            command: false,
        },
    );
    assert!(editor.text().contains("- [x] task"));
    assert_eq!(editor.selection().head, bold);
}

#[test]
fn zoom_sizes_both_panes() {
    let mut editor = side_by_side("text\n".into());
    editor.set_zoom(2.0);
    assert_eq!(editor.preview.lines.borrow().zoom, 2.0);
    assert!(editor.preview.lines.borrow().width < 500.0 - 2.0 * 2.25 * 16.0);
}
