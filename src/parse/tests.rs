//! The parser's events, tags, headings and links, and the spec examples.
use serde::Deserialize;

use super::{events, heading};

#[test]
fn the_first_atx_heading_outside_code_and_quotes_is_the_title() {
    fn title(text: &str) -> Option<&str> {
        heading(text).map(|r| &text[r])
    }
    assert_eq!(title("# Plan\n"), Some("Plan"));
    assert_eq!(title("intro\n\n## Plan ##\n"), Some("Plan"));
    assert_eq!(title("#\tTabbed\n"), Some("Tabbed"));
    assert_eq!(title("# Learning C#\n"), Some("Learning C#"));
    assert_eq!(title("# *Big* plan\n"), Some("*Big* plan"));
    assert_eq!(title("```sh\n# install\n```\n# Real\n"), Some("Real"));
    assert_eq!(title("> # Quoted\n- # Listed\n#\n# Last\n"), Some("Last"));
    assert_eq!(title("Setext\n===\n"), None);
    assert_eq!(title("---\ntitle: x\n---\n# After\n"), Some("After"));
    assert_eq!(title("#hashtag only\n"), None);
}

#[derive(Deserialize)]
struct Example {
    markdown: String,
    html: String,
    example: u32,
    section: String,
}

/// The examples that render differently from the spec's HTML.
fn failures(file: &str) -> Vec<String> {
    let path = format!("{}/tests/fixtures/spec/{file}", env!("CARGO_MANIFEST_DIR"));
    let examples: Vec<Example> =
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    examples
        .iter()
        .filter(|e| {
            let mut html = String::new();
            pulldown_cmark::html::push_html(&mut html, events(&e.markdown).map(|(e, _)| e));
            normalize(&html) != normalize(&e.html)
        })
        .map(|e| format!("{} {}", e.example, e.section))
        .collect()
}

/// Spellings where pulldown-cmark's HTML writer differs from the spec's
/// without parsing differently: `"` unescaped, table alignment as a
/// style, an empty `<tbody>`, checkbox attribute order, and newlines
/// next to tags.
fn normalize(html: &str) -> String {
    let mut html = html.replace("&quot;", "\"");
    for (from, to) in [
        (" style=\"text-align: left\"", " align=\"left\""),
        (" style=\"text-align: center\"", " align=\"center\""),
        (" style=\"text-align: right\"", " align=\"right\""),
        (
            "<input disabled=\"\" type=\"checkbox\" checked=\"\"/>",
            "<input checked=\"\" disabled=\"\" type=\"checkbox\">",
        ),
        (
            "<input disabled=\"\" type=\"checkbox\"/>",
            "<input disabled=\"\" type=\"checkbox\">",
        ),
        ("type=\"checkbox\"> ", "type=\"checkbox\">"),
        (">\n", ">"),
        ("\n<", "<"),
        ("<tbody></tbody>", ""),
    ] {
        html = html.replace(from, to);
    }
    html.trim().to_owned()
}

#[test]
fn every_commonmark_example_parses_like_the_spec_but_bare_urls() {
    // GFM's extended autolinks link what these three CommonMark examples
    // say is plain text (comrak with GFM on differs the same way), and
    // a document starting `---`, a line, `---` is YAML front matter
    // (Obsidian's properties), not a rule and a setext heading.
    assert_eq!(
        failures("commonmark-0.31.2.json"),
        [
            "96 Setext headings",
            "608 Autolinks",
            "611 Autolinks",
            "612 Autolinks"
        ]
    );
}

#[test]
fn tags_are_words_after_a_hash_in_text_only() {
    let text = "---\ntags: [a]\n#meta\n---\n# Heading\n#travel and #work/2026, not#this\n\
                #12 \\#esc `#code` [#link](u#frag) <https://x.org/#top>\n\
                ```\n#fenced\n```\n- #my_tag_name.\n";
    let names: Vec<&str> = super::tags(text).into_iter().map(|r| &text[r]).collect();
    assert_eq!(names, ["#travel", "#work/2026", "#my_tag_name"]);
    assert_eq!(super::front_matter_text(text), Some("tags: [a]\n#meta\n"));
    assert_eq!(super::front_matter_text("no front matter"), None);
    let links =
        super::links("[a](x.md) ![i](p.png) <https://u.v> www.w.x [r][ref]\n\n[ref]: y.md\n");
    assert_eq!(links, ["x.md", "y.md"]);
}

#[test]
fn front_matter_is_one_block_only_at_the_start() {
    use pulldown_cmark::{Tag, TagEnd};
    let block = |text: &str| -> Option<std::ops::Range<usize>> {
        events(text)
            .find(|(e, _)| matches!(e, pulldown_cmark::Event::Start(Tag::MetadataBlock(_))))
            .map(|(_, r)| r)
    };
    assert_eq!(block("---\ntags: [a]\n---\n\n# T\n"), Some(0..17));
    assert_eq!(block("---\r\na: 1\r\n...\r\nx"), Some(0..14), "CRLF, `...`");
    assert_eq!(block("---\n---\n"), None, "two rules");
    assert_eq!(block("---\n\na: 1\n---\n"), None, "a blank first line");
    assert_eq!(block("---\ra: 1\r---\rx"), Some(0..12), "lone CR");
    assert_eq!(block("x\n\n---\na: 1\n---\n"), None, "not at the start");
    assert_eq!(block("---\na: 1\n"), None, "never closed");
    assert_eq!(
        block("\u{feff}---\na: 1\n---\nx"),
        Some(0..15),
        "after a BOM"
    );
    // After it, the rest parses as if it were not there, offsets kept.
    let text = "---\na: 1\n---\n# Title\n";
    let heading = events(text)
        .find(|(e, _)| matches!(e, pulldown_cmark::Event::Start(Tag::Heading { .. })))
        .map(|(_, r)| r);
    assert_eq!(heading, Some(13..21));
    assert!(
        events(text)
            .any(|(e, _)| matches!(e, pulldown_cmark::Event::End(TagEnd::MetadataBlock(_))))
    );
}

#[test]
fn every_gfm_extension_example_parses_like_the_spec() {
    assert_eq!(failures("gfm-0.29-extensions.json"), Vec::<String>::new());
}
