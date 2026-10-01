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
