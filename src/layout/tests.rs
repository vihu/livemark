//! The projection of one line, by hand; `tests/projection_fuzz.rs` checks
//! the round trips on random documents.
use super::{Affinity, Line};
use crate::style::{Style, Styled};

#[test]
fn a_heading_line_without_its_markers() {
    let text = "## Title ##\nnext";
    let styled = Styled::new(text);
    let line = Line::new(text, 0..11, &styled.hidden(14..14), styled.runs());
    assert_eq!(line.text, "Title");
    let style = Style {
        heading: 2,
        ..Style::default()
    };
    assert_eq!(line.runs, [(0..5, style)]);
    // Source 0 and 3 both sit before the T; 8 and 11 both after the e.
    assert_eq!((line.to_display(0), line.to_display(3)), (0, 0));
    assert_eq!((line.to_display(8), line.to_display(11)), (5, 5));
    assert_eq!(line.to_display(1), 0, "inside a hidden run: where it was");
    assert_eq!(line.to_source(0, Affinity::Before), 0);
    assert_eq!(line.to_source(0, Affinity::After), 3);
    assert_eq!(line.to_source(5, Affinity::Before), 8);
    assert_eq!(line.to_source(5, Affinity::After), 11);
    assert_eq!(line.to_source(2, Affinity::Before), 5);
}

#[test]
fn hidden_runs_in_the_middle_join_the_text_around_them() {
    let text = "a **b** c";
    let styled = Styled::new(text);
    let line = Line::new(text, 0..9, &styled.hidden(0..0), styled.runs());
    // The caret at 0 does not touch `**b**` (2..7).
    assert_eq!(line.text, "a b c");
    assert_eq!(line.to_source(2, Affinity::Before), 2);
    assert_eq!(line.to_source(2, Affinity::After), 4);
    assert_eq!(line.to_source(3, Affinity::Before), 5);
    assert_eq!(line.to_source(3, Affinity::After), 7);
    assert_eq!(line.to_display(6), 3);
    let strong = Style {
        strong: true,
        ..Style::default()
    };
    assert_eq!(line.runs, [(2..3, strong)]);
}

#[test]
fn nothing_hidden_is_the_identity() {
    let text = "plain é line";
    let line = Line::new(text, 0..text.len(), &[], &[]);
    assert_eq!(line.text, text);
    for i in (0..=text.len()).filter(|&i| text.is_char_boundary(i)) {
        assert_eq!(line.to_display(i), i);
        assert_eq!(line.to_source(i, Affinity::Before), i);
        assert_eq!(line.to_source(i, Affinity::After), i);
    }
}
