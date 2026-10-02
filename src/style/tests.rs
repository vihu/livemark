//! Styling and the reveal rule (REFERENCE-001 sections 2 to 4), and
//! agreement with the parser over every spec example.
use std::ops::Range;

use pulldown_cmark::Event;
use serde::Deserialize;

use super::{Align, Construct, MarkKind, Style, Styled, Syntax};
use crate::parse;

fn heading(level: u8, marker: bool) -> Style {
    Style {
        heading: level,
        marker,
        ..Style::default()
    }
}

#[test]
fn an_atx_heading_hides_its_opening_and_closing_runs() {
    let styled = Styled::new("## Title ##\nbody\n");
    assert_eq!(
        styled.constructs(),
        [Construct {
            syntax: Syntax::Heading(2),
            range: 0..11,
            markers: [0..3, 8..11],
            group: 0,
            hides: true,
        }]
    );
    assert_eq!(
        styled.runs(),
        [
            (0..3, heading(2, true)),
            (3..8, heading(2, false)),
            (8..11, heading(2, true)),
        ]
    );
    assert_eq!(
        styled.hidden(13..13),
        [0..3, 8..11],
        "caret on the body line"
    );
    assert!(styled.hidden(5..5).is_empty(), "caret on the heading");
    assert!(
        styled.hidden(11..11).is_empty(),
        "caret at its end touches it"
    );
    assert_eq!(
        styled.hidden(12..12),
        [0..3, 8..11],
        "the next line does not"
    );
}

#[test]
fn a_heading_with_no_text_keeps_its_hashes() {
    for text in ["#\n", "# \n", "### ###\n"] {
        let styled = Styled::new(text);
        assert!(styled.constructs().is_empty(), "{text:?}");
        assert_eq!(
            styled.runs()[0].1,
            heading(1 + 2 * (text.len() > 3) as u8, true)
        );
    }
}

#[test]
fn a_setext_underline_is_a_marker_but_stays() {
    let styled = Styled::new("Title\r\n===\r\n");
    assert_eq!(
        styled.runs(),
        [(0..7, heading(1, false)), (7..10, heading(1, true))]
    );
    assert!(styled.constructs().is_empty());
}

#[test]
fn nested_spans_reveal_together_from_the_outermost() {
    let text = "***x*** and `c`";
    let styled = Styled::new(text);
    let summary: Vec<_> = styled
        .constructs()
        .iter()
        .map(|c| (c.syntax, c.markers.clone(), c.group))
        .collect();
    assert_eq!(
        summary,
        [
            (Syntax::Emphasis, [0..1, 6..7], 0),
            (Syntax::Strong, [1..3, 4..6], 0),
            (Syntax::Code, [12..13, 14..15], 2),
        ]
    );
    assert_eq!(styled.hidden(9..9), [0..3, 4..7, 12..13, 14..15]);
    // Right after the outer span touches it; the code span stays hidden.
    assert_eq!(styled.hidden(7..7), [12..13, 14..15]);
    // A selection over both reveals both.
    assert!(styled.hidden(5..13).is_empty());
}

#[test]
fn an_escaped_delimiter_inside_emphasis_is_text() {
    let styled = Styled::new("*a\\*b* ~~s~~");
    let markers: Vec<_> = styled
        .constructs()
        .iter()
        .map(|c| c.markers.clone())
        .collect();
    assert_eq!(
        markers,
        [[0..1, 5..6], [2..3, 4..4], [7..9, 10..12]],
        "the escape's backslash is a marker of its own"
    );
}

#[test]
fn an_escape_hides_its_backslash_until_touched() {
    let text = "a \\* b `\\*` \\q c\\\\";
    let styled = Styled::new(text);
    let hidden = |at: usize| -> Vec<&str> {
        styled
            .hidden(at..at)
            .iter()
            .map(|r| &text[r.clone()])
            .collect()
    };
    // Not in code, not before a letter, not an escaped backslash's second.
    assert_eq!(hidden(0), ["\\", "`", "`", "\\"], "away from it");
    assert_eq!(hidden(2), ["`", "`", "\\"], "touching the escape");
}

#[derive(Deserialize)]
struct Example {
    markdown: String,
}

fn examples(file: &str) -> Vec<String> {
    let path = format!("{}/tests/fixtures/spec/{file}", env!("CARGO_MANIFEST_DIR"));
    let examples: Vec<Example> =
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    examples.into_iter().map(|e| e.markdown).collect()
}

#[test]
fn markers_are_syntax_the_parser_never_calls_text() {
    let coverage = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/docs/coverage.md"
    ))
    .unwrap();
    let all = examples("commonmark-0.31.2.json")
        .into_iter()
        .chain(examples("gfm-0.29-extensions.json"))
        .chain([coverage]);
    let mut checked = 0;
    for markdown in all {
        let styled = Styled::new(&markdown);
        let leaves: Vec<(Range<usize>, bool)> = parse::events(&markdown)
            .filter_map(|(event, range)| match event {
                Event::Code(_) => Some((range, true)),
                Event::Text(_)
                | Event::SoftBreak
                | Event::HardBreak
                | Event::InlineHtml(_)
                | Event::Html(_) => Some((range, false)),
                _ => None,
            })
            .collect();
        for c in styled.constructs() {
            // A link's closing marker holds its destination: any bytes.
            let syntax: Option<&[u8]> = match c.syntax {
                Syntax::Heading(_) => Some(b"# \t"),
                Syntax::Emphasis | Syntax::Strong => Some(b"*_"),
                Syntax::Strikethrough => Some(b"~"),
                Syntax::Code => Some(b"`"),
                Syntax::Link => None,
                Syntax::Escape => Some(b"\\"),
            };
            for m in c.markers.iter().filter(|m| !m.is_empty()) {
                assert!(
                    syntax.is_none_or(|s| markdown[m.clone()].bytes().all(|b| s.contains(&b))),
                    "{markdown:?}: marker {m:?} of {c:?}"
                );
                for (leaf, code) in &leaves {
                    let own = *code && c.syntax == Syntax::Code && *leaf == c.range;
                    assert!(
                        own || leaf.end <= m.start || m.end <= leaf.start,
                        "{markdown:?}: marker {m:?} of {c:?} covers text {leaf:?}"
                    );
                }
                checked += 1;
            }
        }
    }
    assert!(checked > 400, "only {checked} markers checked");
}

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
fn no_dimmed_marker_covers_what_the_parser_calls_text() {
    let coverage = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/docs/coverage.md"
    ))
    .unwrap();
    let all = examples("commonmark-0.31.2.json")
        .into_iter()
        .chain(examples("gfm-0.29-extensions.json"))
        .chain([coverage]);
    for markdown in all {
        let styled = Styled::new(&markdown);
        let texts: Vec<Range<usize>> = parse::events(&markdown)
            .filter(|(event, _)| matches!(event, Event::Text(_)))
            .map(|(_, range)| range)
            .collect();
        for (run, _) in styled.runs().iter().filter(|(_, style)| style.marker) {
            for text in &texts {
                assert!(
                    text.end <= run.start || run.end <= text.start,
                    "{markdown:?}: marker {run:?} covers text {text:?}"
                );
            }
        }
    }
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

/// The text of every run with `pick` set, joined.
fn picked(text: &str, styled: &Styled, pick: fn(&Style) -> bool) -> Vec<String> {
    styled
        .runs()
        .iter()
        .filter(|(_, style)| pick(style))
        .map(|(r, _)| text[r.clone()].to_owned())
        .collect()
}

#[test]
fn links_show_their_text_and_hide_the_rest_unless_touched() {
    let text = "[t **b**](u \"x\") [r][id] <http://a.b> [](e)\n\n[id]: /u\n";
    let styled = Styled::new(text);
    let links: Vec<_> = styled
        .constructs()
        .iter()
        .filter(|c| c.syntax == Syntax::Link)
        .map(|c| c.markers.clone().map(|m| text[m].to_owned()))
        .collect();
    assert_eq!(
        links,
        [
            ["[".to_owned(), "](u \"x\")".to_owned()],
            ["[".into(), "][id]".into()],
            ["<".into(), ">".into()],
            ["".into(), "".into()],
        ],
        "an empty link keeps everything"
    );
    assert_eq!(
        picked(text, &styled, |s| s.link && !s.marker),
        ["t ", "b", "r", "http://a.b"]
    );
    let hidden = styled.hidden(text.len()..text.len());
    assert!(hidden.iter().all(|h| !text[h.clone()].contains("](e)")));
    // The strong inside the link reveals with it (outermost span decides).
    assert!(styled.hidden(2..2).iter().all(|h| h.start > 16));
}

#[test]
fn list_markers_dim_task_boxes_and_done_text_mute_and_rows_hang() {
    let text = "- a\n  - [x] done\n10. ten\n- [ ] open\n";
    let styled = Styled::new(text);
    assert_eq!(
        picked(text, &styled, |s| s.marker),
        ["-", "-", "[x]", "10.", "-", "[ ]"]
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
fn bare_urls_are_link_text_without_markers() {
    let text = "see www.example.com, https://a.b/c and me@x.yz.";
    let styled = Styled::new(text);
    assert_eq!(
        picked(text, &styled, |s| s.link),
        ["www.example.com", "https://a.b/c", "me@x.yz"]
    );
    assert!(styled.constructs().is_empty(), "nothing to hide");
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
            ("*", MarkKind::Bullet),
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
