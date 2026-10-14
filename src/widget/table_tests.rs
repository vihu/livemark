//! Tables as grids: column widths, wrapped cells and row heights.
use super::Editor;
use super::table::fit;

#[test]
fn columns_take_their_natural_width_then_their_words_and_a_share() {
    assert_eq!(fit(&[40.0, 40.0], &[60.0, 80.0], 200.0), [60.0, 80.0]);
    // 200 left over the narrowest, wanted 400 more: half each wants.
    assert_eq!(fit(&[50.0, 40.0], &[50.0, 440.0], 290.0), [50.0, 240.0]);
    assert_eq!(fit(&[100.0, 100.0], &[200.0, 200.0], 100.0), [50.0, 50.0]);
}

#[test]
fn a_long_cell_wraps_and_a_short_column_stays_whole() {
    let words: Vec<String> = (1..=40).map(|n| format!("w{n:02}")).collect();
    let text = format!(
        "| Repo | Change |\n| - | - |\n| erlang-frost | {} |\n\nend",
        words.join(" ")
    );
    let mut editor = Editor::new(text.clone());
    editor.lines.borrow_mut().width = 400.0;
    editor.select(text.len(), text.len());
    let (row, line, grid) = editor.with_lines(|lines, source| {
        let grid = lines.grid(source, 0);
        let top = lines.top_of(source, 2).expect("on screen");
        (top, lines.shaped(source, 2).height, grid)
    });
    assert!(grid.width() <= 400.0, "fits: {}", grid.width());
    let repo = &grid.cells[1][0];
    assert_eq!(repo.buffer.layout_runs().count(), 1, "one row");
    assert!(repo.size.width < grid.columns[0].1, "not clipped");
    let change = &grid.cells[1][1];
    let rows: Vec<_> = change.buffer.layout_runs().collect();
    assert!(rows.len() > 1, "wraps");
    assert!(change.size.width < grid.columns[1].1, "inside its column");
    assert_eq!(line, grid.heights[1], "the line is the grid row's height");
    assert!(grid.heights[1] > grid.heights[0], "taller than the header");
    // A click on the second row's first word lands before it.
    let glyph = &rows[1].glyphs[0];
    let word = &rows[1].text[glyph.start..glyph.start + 3];
    // After the cell's padding, 8 at 100%.
    let x = grid.columns[1].0 + 8.0 + glyph.x + 1.0;
    let y = row + (grid.heights[1] - change.size.height) / 2.0 + rows[1].line_top + 2.0;
    let offset = editor.with_lines(|lines, source| {
        let x0 = lines.grid_x(source, 0);
        lines.table_hit(source, x0 + x, y)
    });
    let offset = offset.expect("on the grid");
    assert!(text[offset..].starts_with(word), "{word} at {offset}");
}
