//! Cases from a first-time-user walkthrough (TASK-001 slice 55).
use super::{Editor, Input, Message};

/// A press and release at `at`, one click, no modifier.
fn click(editor: &mut Editor, at: iced::Point) {
    let press = Input::Press {
        at,
        shift: false,
        clicks: 1,
        command: false,
        other: false,
    };
    let _ = editor.update(Message(press));
    let _ = editor.update(Message(Input::Release));
}

#[test]
fn a_click_right_of_a_line_lands_after_its_hidden_markers() {
    for text in [
        "para\nsome **bold**\n",
        "para\nsome *it*\n",
        "para\na [link](url)\n",
    ] {
        let mut editor = Editor::new(text.into());
        editor.select(0, 0);
        let top = editor.with_lines(|lines, source| lines.top_of(source, 1).unwrap());
        click(&mut editor, iced::Point::new(600.0, top + 10.0));
        assert_eq!(editor.selection().head, text.len() - 1, "{text:?}");
    }
}

#[test]
fn code_and_accent_colors_read_on_both_themes() {
    for theme in [iced::Theme::Light, iced::Theme::Dark] {
        let palette = theme.palette();
        let band = palette.background.weak.color;
        let text = palette.background.base.text;
        for color in [
            palette.success.base.color,
            palette.primary.base.color,
            palette.warning.base.color,
            palette.danger.base.color,
        ] {
            let shown = super::shape::readable(color, band, text);
            assert!(shown.relative_contrast(band) >= 4.5, "{theme:?} {color:?}");
        }
    }
}

#[test]
fn scrolling_to_the_end_stops_with_the_last_line_at_the_bottom() {
    let text = format!("{}last\n", "line\n".repeat(200));
    for input in [Input::Scroll(1e6), Input::ScrollTo(1.0)] {
        let mut editor = Editor::new(text.clone());
        editor.select(0, 0);
        let _ = editor.update(Message(input));
        editor.with_lines(|lines, source| {
            let last = source.doc.line_count() - 1;
            let top = lines.top_of(source, last).expect("on screen");
            let bottom = top + lines.shaped(source, last).height;
            assert!(
                (bottom - lines.height).abs() < 0.5,
                "{bottom} vs {}",
                lines.height
            );
        });
    }
    // A document shorter than the view does not scroll.
    let mut editor = Editor::new("short\n".into());
    let _ = editor.update(Message(Input::Scroll(500.0)));
    assert_eq!(editor.lines.borrow().offset, 0.0);
}

#[test]
fn later_lines_of_a_list_item_line_up_with_its_text() {
    for (text, first, later) in [
        ("para\n\n- item\n  more\n", "item", "more"),
        ("para\n\n1. item\n   more\n", "item", "more"),
        ("para\n\n10. item\n    more\n", "item", "more"),
        ("para\n\n- [ ] task\n  more\n", "task", "more"),
        ("para\n\n- item\n\n  later paragraph\n", "item", "later"),
        ("para\n\n- item\nlazy\n", "item", "lazy"),
        ("para\n\n- item\n\tmore\n", "item", "more"),
        ("para\n\n- item\n      more\n", "item", "more"),
    ] {
        let mut editor = Editor::new(text.into());
        editor.select(0, 0);
        let (first, later) = (text.find(first).unwrap(), text.find(later).unwrap());
        editor.with_lines(|lines, source| {
            let x = |lines: &mut super::lines::Lines, at| {
                lines
                    .caret_in_line(source, at, crate::layout::Affinity::After)
                    .0
            };
            let (first, later) = (x(lines, first), x(lines, later));
            assert!((later - first).abs() < 0.5, "{text:?}: {later} vs {first}");
        });
    }
}

#[test]
fn the_caret_on_a_blank_line_in_an_item_stands_where_text_will_go() {
    // As Shift+Enter leaves it: typing there must not move the text.
    for (text, first) in [
        ("para\n\n- [ ] task\n      ", "task"),
        ("para\n\n10. item\n    ", "item"),
        ("para\n\n- a\n  - item\n    ", "item"),
    ] {
        let typed = format!("{text}x");
        let x = |text: &str, at: usize| {
            let mut editor = Editor::new(text.into());
            editor.select(0, 0);
            editor.with_lines(|lines, source| {
                lines
                    .caret_in_line(source, at, crate::layout::Affinity::After)
                    .0
            })
        };
        let item = x(text, text.find(first).unwrap());
        let blank = x(text, text.len());
        let after = x(&typed, text.len());
        assert!((blank - item).abs() < 0.5, "{text:?}: {blank} vs {item}");
        assert!((after - item).abs() < 0.5, "{text:?}: {after} vs {item}");
    }
}

#[test]
fn the_scroll_bar_at_its_bottom_shows_the_end_of_mixed_lines() {
    // Short lines, then long paragraphs; the last frame drew the top.
    let long = "word ".repeat(80);
    let text = format!(
        "{}{}",
        "- item\n".repeat(30),
        format!("{long}\n").repeat(12)
    );
    let mut editor = Editor::new(text);
    editor.select(0, 0);
    editor.lines.borrow_mut().visible = 15;
    let _ = editor.update(Message(Input::ScrollTo(1.0)));
    editor.with_lines(|lines, source| {
        let last = source.doc.line_count() - 1;
        let top = lines.top_of(source, last).expect("on screen");
        let bottom = top + lines.shaped(source, last).height;
        assert!(
            (bottom - lines.height).abs() < 0.5,
            "{bottom} vs {}",
            lines.height
        );
    });
}
