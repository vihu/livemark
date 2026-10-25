//! Tables as grids: column widths, wrapped cells, row heights, and clicks
//! and links in their cells.
use super::Editor;
use super::table::fit;
use super::tests::outputs;

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
    // Within float rounding: the shares' sum can land a hair over.
    assert!(grid.width() <= 400.01, "fits: {}", grid.width());
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

#[test]
fn a_click_on_a_grid_cell_puts_the_caret_in_its_source() {
    use super::{Input, Message};
    let text = "x\n\n| a | bb |\n| - | - |\n| c | **dd** |\n";
    let mut editor = Editor::new(text.into());
    editor.select(text.len(), text.len());
    // The left edge of row 2's second column, a little in.
    let at = editor.with_lines(|lines, source| {
        let grid = lines.grid(source, 0);
        let top = lines.top_of(source, 4).expect("on screen");
        let shaped = lines.shaped(source, 4);
        let start = super::marks::start_x(&shaped, source.doc.line_range(4).start);
        iced::Point::new(start + grid.columns[1].0 + 9.0, top + 5.0)
    });
    let _ = editor.update(Message(Input::Press {
        at,
        shift: false,
        clicks: 1,
        command: false,
        other: false,
    }));
    let _ = editor.update(Message(Input::Release));
    let head = editor.selection().head;
    assert_eq!(&text[head..head + 2], "dd", "after the hidden `**`");
    // Text typed above moves the table; its grid is shared by text, so a
    // click must still land in the moved source.
    editor.select(0, 0);
    let _ = editor.update(Message(Input::Key(super::Key::Insert('z'))));
    let end = editor.text().len();
    editor.select(end, end);
    let _ = editor.update(Message(Input::Press {
        at,
        shift: false,
        clicks: 1,
        command: false,
        other: false,
    }));
    let _ = editor.update(Message(Input::Release));
    let head = editor.selection().head;
    assert_eq!(&editor.text()[head..head + 2], "dd", "after an edit above");
}

#[test]
fn a_grid_starts_where_its_rows_source_starts() {
    let text = "- | a |\n  | - |\n  | b |\n\nend";
    let mut editor = Editor::new(text.into());
    editor.select(text.len(), text.len());
    editor.with_lines(|lines, source| {
        let shaped = lines.shaped(source, 0);
        let x = super::marks::start_x(&shaped, 2);
        assert!(x > 5.0, "after the bullet: {x}");
    });
}

#[test]
fn a_cached_grid_follows_a_link_definition_added_elsewhere() {
    use super::{Input, Key, Message};
    let mut editor = Editor::new("| [a] |\n| - |\n\n".into());
    let cell = |editor: &Editor| {
        editor.with_lines(|lines, source| lines.grid(source, 0).cells[1 - 1][0].line.text.clone())
    };
    let end = editor.text().len();
    editor.select(end, end);
    assert_eq!(cell(&editor), "[a]", "no definition: text");
    for c in "[a]: http://x.y".chars() {
        let _ = editor.update(Message(Input::Key(Key::Insert(c))));
    }
    assert_eq!(cell(&editor), "a", "a reference link now");
}

#[test]
fn a_grid_follows_links_only_on_cell_text_and_its_rule_maps_by_column() {
    use super::{Input, Message};
    let text = "> | a | [l](http://x.y) |\n> | - | - |\n> | b | c |\n\nend\n";
    let mut editor = Editor::new(text.into());
    editor.select(text.len(), text.len());
    let (x0, second, top0, top1) = editor.with_lines(|lines, source| {
        let grid = lines.grid(source, 0);
        let x0 = lines.grid_x(source, 0);
        let top0 = lines.top_of(source, 0).unwrap();
        let top1 = lines.top_of(source, 1).unwrap();
        (x0, grid.columns[1].0, top0, top1)
    });
    let press = |at: iced::Point| {
        Message(Input::Press {
            at,
            shift: false,
            clicks: 1,
            command: true,
            other: false,
        })
    };
    // Far right of the grid, on the link's row: nothing to follow.
    let task = editor.update(press(iced::Point::new(x0 + second + 150.0, top0 + 8.0)));
    assert!(outputs(task).iter().all(|m| m.link().is_none()));
    // On the link's text it follows.
    let task = editor.update(press(iced::Point::new(x0 + second + 10.0, top0 + 8.0)));
    assert_eq!(
        outputs(task)
            .iter()
            .filter_map(|m| m.link())
            .collect::<Vec<_>>(),
        ["http://x.y"]
    );
    // A click on the rule under the header goes to the header's cell in
    // that column.
    let offset =
        editor.with_lines(|lines, source| lines.table_hit(source, x0 + second + 2.0, top1 + 1.0));
    let offset = offset.expect("on the grid");
    let cell = editor.styled.tables()[0].rows[0].1[1].clone();
    assert!(
        cell.contains(&offset),
        "{offset} in the header's second cell {cell:?}"
    );
}
