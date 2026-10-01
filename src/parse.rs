//! Markdown parsing: CommonMark 0.31.2 plus GFM tables, strikethrough and
//! task lists, through pulldown-cmark. The whole document is parsed again
//! on every edit: 0.4 ms for 5,000 lines (PLAN-001 decisions log), so no
//! incremental parser is needed.
use std::ops::Range;

use pulldown_cmark::{Event, Options, Parser};

/// The syntax livemark reads beyond CommonMark. GFM's extended autolinks
/// (`www.` and bare URLs) are not among them yet: pulldown-cmark has none.
pub const OPTIONS: Options = Options::ENABLE_TABLES
    .union(Options::ENABLE_STRIKETHROUGH)
    .union(Options::ENABLE_TASKLISTS);

/// The document's parse events with the source range of each: a start
/// or end event spans its whole construct, markers included.
pub fn events(text: &str) -> impl Iterator<Item = (Event<'_>, Range<usize>)> {
    Parser::new_ext(text, OPTIONS).into_offset_iter()
}

#[cfg(test)]
mod tests {
    use serde::Deserialize;

    use super::events;

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
    fn every_commonmark_example_parses_like_the_spec() {
        assert_eq!(failures("commonmark-0.31.2.json"), Vec::<String>::new());
    }

    #[test]
    fn gfm_extensions_parse_like_the_spec_except_extended_autolinks() {
        let failures = failures("gfm-0.29-extensions.json");
        assert!(
            failures.iter().all(|f| f.ends_with(" Autolinks")),
            "{failures:?}"
        );
        assert_eq!(failures.len(), 11, "the 11 autolink examples: {failures:?}");
    }
}
