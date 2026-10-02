//! Inline `#tags` (PLAN-004): styled from the parse, and Ctrl/Cmd+click or
//! Alt+Enter on one hands its name to the host, in either pane.
use iced::Point;

use super::tests::outputs;
use super::{Editor, Input, Key, Message, Mode, Pane};
use crate::layout::Affinity;

fn tags(task: iced::Task<Message>) -> Vec<String> {
    outputs(task)
        .iter()
        .filter_map(|m| m.tag().map(str::to_owned))
        .collect()
}

fn press(at: Point, command: bool) -> Message {
    Message(Input::Press {
        at,
        shift: false,
        clicks: 1,
        command,
        other: false,
    })
}

#[test]
fn tags_are_styled_in_text_only() {
    let text = "#travel and `#code` [#x](u) #12 \\#esc #work/2026\n";
    let styled = crate::style::Styled::new(text);
    assert_eq!(styled.tag_at(3), Some((0..7, "travel")));
    assert_eq!(styled.tag_at(14), None, "not in code");
    assert_eq!(styled.tag_at(22), None, "not in a link");
    assert_eq!(styled.tag_at(29), None, "not all digits");
    let work = text.find("#work").unwrap();
    assert_eq!(styled.tag_at(work + 2).map(|t| t.1), Some("work/2026"));
    let tagged = styled.runs().iter().filter(|(_, s)| s.tag).count();
    assert_eq!(tagged, 2);
}

#[test]
fn ctrl_click_or_alt_enter_on_a_tag_hands_it_to_the_host() {
    let text = "plan #travel soon\n";
    let mut editor = Editor::new(text.into());
    editor.select(text.len(), text.len());
    let x_of = |editor: &Editor, offset: usize| {
        editor.with_lines(|lines, source| lines.caret_in_line(source, offset, Affinity::After).0)
    };
    let at = Point::new(x_of(&editor, 8) + 1.0, 10.0);
    assert_eq!(tags(editor.update(press(at, true))), ["travel"]);
    assert_eq!(editor.selection().head, text.len(), "the caret stays");
    let _ = editor.update(Message(Input::Release));
    // A plain click puts the caret there.
    assert!(tags(editor.update(press(at, false))).is_empty());
    let _ = editor.update(Message(Input::Release));
    assert_eq!(
        tags(editor.update(Message(Input::Key(Key::Follow)))),
        ["travel"]
    );
    // In the rendered pane too.
    editor.set_mode(Mode::Split);
    for lines in [&editor.lines, &editor.preview.lines] {
        let mut lines = lines.borrow_mut();
        lines.outer = iced::Size::new(500.0, 400.0);
        lines.fit();
        lines.sized = true;
    }
    let at = editor.with_pane(Pane::Preview, |lines, source| {
        Point::new(
            lines.caret_in_line(source, 8, Affinity::After).0 + 1.0,
            10.0,
        )
    });
    let task = editor.update(Message(Input::PreviewPress { at, command: true }));
    assert_eq!(tags(task), ["travel"]);
}
