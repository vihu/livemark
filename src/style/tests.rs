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
                Syntax::Highlight => Some(b"="),
            };
            for m in c.markers.iter().filter(|m| !m.is_empty()) {
                assert!(
                    syntax.is_none_or(|s| markdown[m.clone()].bytes().all(|b| s.contains(&b))),
                    "{markdown:?}: marker {m:?} of {c:?}"
                );
                for (leaf, code) in &leaves {
                    // Obsidian's `==` is found in text, beyond CommonMark.
                    let own = (*code && c.syntax == Syntax::Code && *leaf == c.range)
                        || c.syntax == Syntax::Highlight;
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
        // Obsidian's `==` is found in text, beyond CommonMark.
        let highlights: Vec<Range<usize>> = styled
            .constructs()
            .iter()
            .filter(|c| c.syntax == Syntax::Highlight)
            .flat_map(|c| c.markers.clone())
            .collect();
        for (run, _) in styled.runs().iter().filter(|(run, style)| {
            style.marker
                && !highlights
                    .iter()
                    .any(|h| h.start <= run.start && run.end <= h.end)
        }) {
            for text in &texts {
                assert!(
                    text.end <= run.start || run.end <= text.start,
                    "{markdown:?}: marker {run:?} covers text {text:?}"
                );
            }
        }
    }
}

/// The text of every run with `pick` set, joined.
pub(super) fn picked(text: &str, styled: &Styled, pick: fn(&Style) -> bool) -> Vec<String> {
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
fn highlights_pair_in_a_block_and_hide_like_strong() {
    let text = "a ==mark== b == c ==x== y==\n\n==not\n\nclosed==\n";
    let styled = Styled::new(text);
    let highlighted: Vec<&str> = styled
        .runs()
        .iter()
        .filter(|(_, s)| s.highlight)
        .map(|(r, _)| &text[r.clone()])
        .collect();
    // `== c ==` has blanks inside its pair; a highlight never crosses a
    // blank line.
    assert_eq!(highlighted, ["mark", "x"]);
    let hidden: Vec<&str> = styled
        .hidden(0..0)
        .iter()
        .map(|r| &text[r.clone()])
        .collect();
    assert_eq!(hidden, ["==", "==", "==", "=="]);
    // Touching it shows its markers.
    assert_eq!(styled.hidden(4..4).len(), 2);
    // Other inline spans inside it, and `===` is no marker.
    let nested = Styled::new("==a **b** c== and a===b===c\n");
    assert!(nested.runs().iter().any(|(_, s)| s.highlight && s.strong));
    assert_eq!(
        nested
            .constructs()
            .iter()
            .filter(|c| c.syntax == Syntax::Highlight)
            .count(),
        1
    );
    // A highlight inside emphasis reveals with it.
    let inside = Styled::new("*a ==b== c* d\n");
    assert!(inside.hidden(1..1).is_empty(), "touching the emphasis");
}

#[test]
fn highlights_reveal_what_is_inside_and_skip_urls_and_front_matter() {
    // Touching a highlight shows the markers of the spans inside it.
    let text = "==**Important:** read this==\n";
    let styled = Styled::new(text);
    let read = text.find("read").unwrap();
    assert!(
        styled.hidden(read..read).is_empty(),
        "{:?}",
        styled.hidden(read..read)
    );
    // Not across a span's edge, not in a URL, not in front matter.
    for text in [
        "**==a**==\n",
        "[==a](u) b==\n",
        "see https://a.com/?x==1&y==2 ok\n",
        "<https://a.com/?x==1&y==2>\n",
        "---\na: x==1\nb: y==2\n---\n",
    ] {
        let styled = Styled::new(text);
        assert!(
            styled
                .constructs()
                .iter()
                .all(|c| c.syntax != Syntax::Highlight),
            "{text:?}"
        );
    }
}
