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
        (
            "para\n\n> [!note]\n> - [ ] task\n>   more\n",
            "task",
            "more",
        ),
        ("para\n\n> - item\n>   more\n", "item", "more"),
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
        ("para\n\n> - [ ] item\n>       ", "item"),
    ] {
        let typed = format!("{text}x");
        let x = |text: &str, at: usize| {
            let mut editor = Editor::new(text.into());
            editor.select(0, 0);
            // As the surface gives it: padding and gutter, less the edge.
            editor.lines.borrow_mut().room = 48.0;
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

#[test]
fn zoom_scales_every_line_and_keeps_the_caret_row_in_place() {
    let text = format!(
        "{}# Title\n\n- item\n\n| a | b |\n| - | - |\n| c | d |\n",
        "line\n".repeat(60)
    );
    let mut editor = Editor::new(text.clone());
    let caret = text.find("item").unwrap();
    editor.select(caret, caret);
    let at = |editor: &Editor| {
        editor.with_lines(|lines, source| {
            let index = source.doc.line_at(caret);
            let top = lines.top_of(source, index).expect("on screen");
            let (_, row, height) =
                lines.caret_in_line(source, caret, crate::layout::Affinity::After);
            (top + row, height)
        })
    };
    let _ = editor.update(Message(Input::ScrollTo(1.0)));
    let (y, height) = at(&editor);
    editor.set_zoom(2.0);
    assert_eq!(editor.zoom(), 2.0);
    let (zoomed_y, zoomed) = at(&editor);
    assert!((zoomed - 2.0 * height).abs() < 2.0, "{zoomed} vs {height}");
    assert!(
        (zoomed_y - y).abs() < 0.5,
        "the caret's row stays: {zoomed_y} vs {y}"
    );
    // Clamped, and back.
    editor.set_zoom(10.0);
    assert_eq!(editor.zoom(), 3.0);
    editor.set_zoom(1.0);
    assert!((at(&editor).1 - height).abs() < 0.5);
}

#[test]
fn a_revealed_heading_run_hangs_left_and_the_text_stays() {
    for text in ["# Title\n\npara\n", "### Title\n\npara\n"] {
        let title = text.find("Title").unwrap();
        let mut editor = Editor::new(text.into());
        // As the surface gives it: padding and gutter, less the edge.
        editor.lines.borrow_mut().room = 48.0;
        let x = |editor: &mut Editor, at: usize| {
            editor.with_lines(|lines, source| {
                lines
                    .caret_in_line(source, at, crate::layout::Affinity::After)
                    .0
            })
        };
        // Hidden with the caret in the paragraph, shown with it on the line.
        editor.select(text.len() - 1, text.len() - 1);
        let hidden = x(&mut editor, title);
        editor.select(title + 2, title + 2);
        let shown = x(&mut editor, title);
        assert!(
            (shown - hidden).abs() < 0.5,
            "{text:?}: {shown} vs {hidden}"
        );
        // The run is left of the text, and a click there lands in it.
        let run = x(&mut editor, 0);
        assert!(run < -1.0, "{text:?}");
        let (offset, _) = editor.with_lines(|lines, source| lines.hit(source, run + 2.0, 10.0));
        assert!(offset < title, "{text:?}: {offset}");
    }
}
