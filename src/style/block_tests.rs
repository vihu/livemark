//! Styling of blocks: code blocks and fences, tables, lists and task
//! boxes, quotes, front matter, and the marks concealed until touched
//! (REFERENCE-001 sections 6 to 10).
use super::tests::picked;
use super::{Align, MarkKind, Styled};

#[test]
fn code_blocks_dim_their_fences_and_keep_their_lines() {
    let text = "```rust title\nfn a() {\n\n}\n```\n> ```\n> x\n> ```\n\n    indented\n";
    let styled = Styled::new(text);
    let blocks = styled.code_blocks();
    assert_eq!(blocks.len(), 3);
    assert_eq!(blocks[0].language.as_deref(), Some("rust"));
    assert_eq!(blocks[0].range, 0..29);
    let lines: Vec<&str> = blocks[0].lines.iter().map(|r| &text[r.clone()]).collect();
    assert_eq!(lines, ["fn a() {", "", "}"]);
    let quoted: Vec<&str> = blocks[1].lines.iter().map(|r| &text[r.clone()]).collect();
    assert_eq!(quoted, ["x"], "without the quote's prefix");
    assert_eq!(blocks[2].language, None, "indented");
    let marked: String = styled
        .runs()
        .iter()
        .filter(|(_, style)| style.marker)
        .map(|(r, _)| &text[r.clone()])
        .collect();
    // The quoted block starts after its first `> `, which is the quote's
    // (its `>` dimmed as a quote marker).
    assert_eq!(
        marked, "```rust title\n```>```\n> > ```",
        "fences and prefixes only"
    );
    assert!(
        styled
            .runs()
            .iter()
            .any(|(r, s)| s.code_block && r.start == 14)
    );
}

#[test]
fn a_table_is_monospace_with_its_pipes_and_delimiter_row_dimmed() {
    let text = "| a | b |\n| :- | -: |\n| 1 | 2 |\n";
    let styled = Styled::new(text);
    // Rows, not the line endings between them (in a list or quote the
    // prefixes there would be in the code font too).
    assert!(
        styled
            .runs()
            .iter()
            .all(|(r, s)| s.table || &text[r.clone()] == "\n")
    );
    let marked: Vec<&str> = styled
        .runs()
        .iter()
        .filter(|(_, style)| style.marker)
        .map(|(r, _)| &text[r.clone()])
        .collect();
    assert_eq!(
        marked,
        ["|", "|", "|", "\n", "| :- | -: |", "\n", "|", "|", "|"]
    );
}

#[test]
fn a_table_in_a_list_keeps_its_rows_indentation_in_the_prose_font() {
    let text = "- x\n\n  | a |\n  | - |\n  | b |\n";
    let styled = Styled::new(text);
    let table = picked(text, &styled, |s| s.table);
    // No run starts with the indentation; the header's text is bold, so
    // its padding is a run of its own.
    assert_eq!(table, ["|", " ", "a", " ", "|", "| - |", "|", " b ", "|"]);
}

#[test]
fn inline_markers_in_a_table_stay_so_columns_line_up() {
    let text = "| **b** | `c` |\n| - | - |\n\n**d**\n";
    let styled = Styled::new(text);
    let hidden = styled.hidden(text.len()..text.len());
    let shown: Vec<&str> = hidden.iter().map(|r| &text[r.clone()]).collect();
    assert_eq!(shown, ["**", "**"], "only the paragraph's, after the table");
    assert!(hidden.iter().all(|r| r.start > 24));
}

#[test]
fn list_markers_dim_task_boxes_and_done_text_mute_and_rows_hang() {
    let text = "- a\n  - [x] done\n10. ten\n- [ ] open\n";
    let styled = Styled::new(text);
    assert_eq!(
        picked(text, &styled, |s| s.marker),
        // A task's bullet and the space up to its box dim together.
        ["-", "-", " ", "[x]", "10.", "-", " ", "[ ]"]
    );
    assert_eq!(picked(text, &styled, |s| s.done), ["done"]);
    let hang = |line: std::ops::Range<usize>| styled.hang_at(line);
    assert_eq!(hang(0..3), Some(2));
    assert_eq!(hang(4..16), Some(12), "after the task box");
    assert_eq!(hang(17..24), Some(21));
}

#[test]
fn quotes_dim_every_marker_and_hang_after_them() {
    let text = "> q\n> > n\nlazy\n\nafter\n";
    let styled = Styled::new(text);
    assert_eq!(picked(text, &styled, |s| s.marker), [">", ">", ">"]);
    assert_eq!(styled.hang_at(4..9), Some(8), "after both markers");
    assert!(styled.in_quote(10..14), "a lazy line");
    assert!(!styled.in_quote(16..21));
    let rule = Styled::new("a\n\n---\n");
    assert_eq!(picked("a\n\n---\n", &rule, |s| s.marker), ["---"]);
}

#[test]
fn bullets_boxes_quote_marks_and_rules_conceal_until_touched() {
    let text = "- a\n  * [x] b\n1. c\n> > q\n\n  ---\n";
    let styled = Styled::new(text);
    let concealed = |at: usize| -> Vec<(&str, MarkKind)> {
        styled
            .concealed(at..at)
            .into_iter()
            .map(|m| (&text[m.range], m.kind))
            .collect()
    };
    let all = concealed(text.len());
    assert_eq!(
        all,
        [
            ("-", MarkKind::Bullet),
            // A task's box stands in for its bullet (section 8).
            ("[x]", MarkKind::Task(true)),
            (">", MarkKind::Quote),
            (">", MarkKind::Quote),
            ("---", MarkKind::Rule),
        ],
        "ordered numbers always show"
    );
    assert_eq!(concealed(2).len(), all.len(), "a caret at the item's text");
    assert!(
        !concealed(1).contains(&("-", MarkKind::Bullet)),
        "touching it"
    );
    assert!(!concealed(9).iter().any(|m| m.1 == MarkKind::Task(true)));
    assert!(
        !concealed(6).iter().any(|m| m.1 == MarkKind::Task(true)),
        "touching its bullet shows the box too"
    );
    assert!(
        !concealed(26).iter().any(|m| m.1 == MarkKind::Rule),
        "a caret at the start of the indented rule line"
    );
}

#[test]
fn fences_and_setext_underlines_conceal_until_their_block_is_touched() {
    let text = "Title\n=====\n\n> ```rust x\n> code\n> ```\n\n```\nopen\n";
    let styled = Styled::new(text);
    let concealed = |at: usize| -> Vec<&str> {
        styled
            .concealed(at..at)
            .into_iter()
            .filter(|m| matches!(m.kind, MarkKind::Fence | MarkKind::Underline))
            .map(|m| &text[m.range])
            .collect()
    };
    assert_eq!(
        concealed(text.len()),
        ["=====", "```rust x", "```", "```"],
        "an unclosed block has its opening fence only"
    );
    assert_eq!(
        concealed(2),
        ["```rust x", "```", "```"],
        "the heading's text"
    );
    assert_eq!(concealed(22), ["=====", "```"], "inside the quoted block");
}

#[test]
fn quoted_fences_split_on_crlf_and_lf() {
    // A lone `\r` after a fence is not a line ending to pulldown-cmark
    // 0.13, so no block is parsed there; the view follows the parser.
    for ending in ["\r\n", "\n"] {
        let text = ["> ```", "> a", "> ```", ""].join(ending);
        let styled = Styled::new(&text);
        let lines: Vec<&str> = styled.code_blocks()[0]
            .lines
            .iter()
            .map(|l| &text[l.clone()])
            .collect();
        assert_eq!(lines, ["a"], "{ending:?}");
        let fences: Vec<&str> = styled
            .concealed(text.len()..text.len())
            .into_iter()
            .filter(|m| m.kind == MarkKind::Fence)
            .map(|m| &text[m.range])
            .collect();
        assert_eq!(fences, ["```", "```"], "{ending:?}");
    }
}

#[test]
fn a_table_records_its_rows_cells_and_alignment_for_the_grid() {
    let text = "> | a | **b** |\r\n> | :- | -: |\r\n> | c |\r\n\nafter\n";
    let styled = Styled::new(text);
    let table = &styled.tables()[0];
    assert_eq!(table.align, [Align::Left, Align::Right]);
    let rows: Vec<(&str, Vec<&str>)> = table
        .rows
        .iter()
        .map(|(row, cells)| {
            let cells = cells.iter().map(|c| &text[c.clone()]).collect();
            (&text[row.clone()], cells)
        })
        .collect();
    assert_eq!(
        rows,
        [
            ("| a | **b** |", vec!["a", "**b**"]),
            ("| c |", vec!["c", ""]),
        ]
    );
    let marks: Vec<(&str, MarkKind)> = styled
        .concealed(text.len()..text.len())
        .into_iter()
        .filter(|m| matches!(m.kind, MarkKind::TableRow(..) | MarkKind::TableRule(_)))
        .map(|m| (&text[m.range], m.kind))
        .collect();
    assert_eq!(
        marks,
        [
            ("| a | **b** |", MarkKind::TableRow(0, 0)),
            ("| :- | -: |", MarkKind::TableRule(0)),
            ("| c |", MarkKind::TableRow(0, 1)),
        ]
    );
    assert!(
        styled
            .concealed(3..3)
            .iter()
            .all(|m| m.touch != table.range),
        "a caret in the table shows all of it"
    );
    assert_eq!(
        picked(text, &styled, |s| s.strong),
        ["a", "**", "b", "**"],
        "the header"
    );
}

#[test]
fn a_lazy_quote_line_lines_up_with_the_quoted_text_before_it() {
    let text = "> quoted text\nlazy line\n\nafter\n";
    let styled = Styled::new(text);
    assert_eq!(styled.lazy_at(14..23), Some(2), "after `> `");
    assert_eq!(styled.lazy_at(0..13), None, "the quoted line itself");
    assert_eq!(styled.lazy_at(25..30), None, "after the quote");
    let text = "> - item\nlazy\n";
    assert_eq!(Styled::new(text).lazy_at(9..13), Some(4), "after `> - `");
}

#[test]
fn every_quote_marker_is_one_after_a_list_marker_and_lone_crs_too() {
    let text = "> - > x\n";
    let styled = Styled::new(text);
    let quotes: Vec<usize> = styled
        .concealed(text.len()..text.len())
        .iter()
        .filter(|m| m.kind == MarkKind::Quote)
        .map(|m| m.range.start)
        .collect();
    assert_eq!(quotes, [0, 4]);
    let text = "a\r\r> b\r> c\r";
    let styled = Styled::new(text);
    let quotes = styled
        .concealed(0..0)
        .iter()
        .filter(|m| m.kind == MarkKind::Quote)
        .count();
    assert_eq!(quotes, 2, "both lines of the quote");
}

#[test]
fn a_task_items_bullet_hides_with_its_box_concealed() {
    let text = "para\n- [ ] task\n";
    let styled = Styled::new(text);
    // The caret elsewhere: `- [` hidden, the box drawn in its place, its
    // `]` laid out as the room before the text.
    let hidden = styled.hidden(0..0);
    assert!(
        hidden.contains(&(5..8)) && !hidden.iter().any(|h| h.contains(&9)),
        "{hidden:?}"
    );
    // Touching the bullet, the box or the space after it shows them.
    for at in [5, 7, 10] {
        assert!(styled.hidden(at..at).iter().all(|h| h.end <= 5), "{at}");
    }
}

#[test]
fn front_matter_is_one_dimmed_code_font_block() {
    let text = "---\ntags: [work]\n---\n\n# Title\n";
    let styled = Styled::new(text);
    let block = 0..text.find("\n\n").unwrap();
    for (range, style) in styled.runs() {
        if range.start < block.end {
            assert!(
                style.meta && !style.marker && style.heading == 0,
                "{range:?}"
            );
        }
    }
    assert!(
        styled
            .concealed(text.len()..text.len())
            .iter()
            .all(|m| m.kind != MarkKind::Rule),
        "its fences are no rules"
    );
    // Further down, `---` is a rule again.
    let later = Styled::new("a\n\n---\nb\n---\n");
    assert!(
        later
            .concealed(0..0)
            .iter()
            .any(|m| m.kind == MarkKind::Rule)
    );
}

#[test]
fn images_in_tables_get_no_picture() {
    let text = "![a](x.png)\n\n| ![b](y.png) | c |\n| - | - |\n";
    let styled = Styled::new(text);
    let urls: Vec<&str> = styled.images().iter().map(|(_, u)| u.as_str()).collect();
    assert_eq!(urls, ["x.png"]);
}

#[test]
fn a_task_box_hides_only_its_own_items_bullet() {
    let text = "- 1. [ ] task\n* 2. [ ] a\n  3. [ ] b\n";
    let styled = Styled::new(text);
    let hidden: Vec<&str> = styled
        .hidden(text.len()..text.len())
        .iter()
        .map(|r| &text[r.clone()])
        .collect();
    // Ordered tasks keep their numbers and the outer bullets: only `[`.
    assert_eq!(hidden, ["[", "[", "["]);
    let bullets = styled
        .concealed(text.len()..text.len())
        .iter()
        .filter(|m| m.kind == MarkKind::Bullet)
        .count();
    assert_eq!(bullets, 2, "the outer bullets' dots stay");
}
