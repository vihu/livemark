//! Styling and the reveal rule (REFERENCE-001 sections 2 to 4), and
//! agreement with the parser over every spec example.
use std::ops::Range;

use pulldown_cmark::Event;
use serde::Deserialize;

use super::{Construct, Style, Styled, Syntax};
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
    assert_eq!(markers, [[0..1, 5..6], [7..9, 10..12]]);
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
            let syntax: &[u8] = match c.syntax {
                Syntax::Heading(_) => b"# \t",
                Syntax::Emphasis | Syntax::Strong => b"*_",
                Syntax::Strikethrough => b"~",
                Syntax::Code => b"`",
            };
            for m in c.markers.iter().filter(|m| !m.is_empty()) {
                assert!(
                    markdown[m.clone()].bytes().all(|b| syntax.contains(&b)),
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
