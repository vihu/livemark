//! Markdown parsing: CommonMark 0.31.2 plus GFM tables, strikethrough, task
//! lists and extended autolinks, through pulldown-cmark (autolinks are
//! livemark's own, `autolink.rs`). The whole document is parsed again on
//! every edit: 0.4 ms for 5,000 lines (PLAN-001 decisions log), so no
//! incremental parser is needed.
mod autolink;

use std::collections::VecDeque;
use std::iter::Peekable;
use std::ops::Range;

use pulldown_cmark::{CowStr, Event, LinkType, OffsetIter, Options, Parser, Tag, TagEnd};

/// The syntax pulldown-cmark reads beyond CommonMark; GFM's extended
/// autolinks are added after it.
pub const OPTIONS: Options = Options::ENABLE_TABLES
    .union(Options::ENABLE_STRIKETHROUGH)
    .union(Options::ENABLE_TASKLISTS);

/// The document's parse events with the source range of each: a start
/// or end event spans its whole construct, markers included. A `www.` link,
/// bare URL or email address in text comes as a link of type
/// [`LinkType::Autolink`] or [`LinkType::Email`] whose range has no `<`.
pub fn events(text: &str) -> impl Iterator<Item = (Event<'_>, Range<usize>)> {
    Events {
        text,
        inner: Parser::new_ext(text, OPTIONS).into_offset_iter().peekable(),
        ready: VecDeque::new(),
        run: Vec::new(),
        code: 0,
        links: 0,
        text_end: 0,
    }
}

/// pulldown-cmark's events with extended autolinks spliced in, lazily:
/// only the current run of text is held back.
struct Events<'a> {
    text: &'a str,
    inner: Peekable<OffsetIter<'a>>,
    /// Events worked out and not yet handed on.
    ready: VecDeque<(Event<'a>, Range<usize>)>,
    /// Adjacent text events (pulldown-cmark splits text at `_`, `&` and
    /// the like), outside code and links, searched together.
    run: Vec<(Event<'a>, Range<usize>)>,
    code: usize,
    links: usize,
    /// The end of the last text, to tell an escaping backslash (in no
    /// text) from a literal one.
    text_end: usize,
}

impl<'a> Iterator for Events<'a> {
    type Item = (Event<'a>, Range<usize>);

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some(event) = self.ready.pop_front() {
                return Some(event);
            }
            match self.inner.next() {
                Some((event, range)) => {
                    if let Some(now) = self.take(event, range) {
                        return Some(now);
                    }
                }
                None if self.run.is_empty() => return None,
                None => self.flush(),
            }
        }
    }
}

impl<'a> Events<'a> {
    /// Takes the next event; returns it when nothing waits before it.
    fn take(&mut self, event: Event<'a>, range: Range<usize>) -> Option<(Event<'a>, Range<usize>)> {
        let text = self.text;
        // An escape or entity (text unlike its source) is never searched
        // and ends a run, as each is its own text node in cmark-gfm.
        let literal = matches!(&event, Event::Text(t) if is_source(t, &text[range.clone()]));
        let searched = literal && self.code == 0 && self.links == 0;
        let escaped = range.start > self.text_end && text[..range.start].ends_with('\\');
        if matches!(event, Event::Text(_) | Event::Code(_)) {
            self.text_end = range.end;
        }
        if searched && escaped {
            // pulldown-cmark starts the text at the escaped character; it
            // is its own text, never part of an autolink.
            self.flush();
            let first = range.start + text[range.start..].chars().next().map_or(0, char::len_utf8);
            self.ready.push_back((
                Event::Text(CowStr::Borrowed(&text[range.start..first])),
                range.start..first,
            ));
            if first < range.end {
                self.run.push((
                    Event::Text(CowStr::Borrowed(&text[first..range.end])),
                    first..range.end,
                ));
            }
            return None;
        }
        // The usual case: a text alone, holding no link, goes straight on.
        let alone = searched
            && self.run.is_empty()
            && self.ready.is_empty()
            && !matches!(self.inner.peek(), Some((Event::Text(_), next)) if next.start == range.end);
        if alone && !maybe_link(&text[range.clone()]) {
            return Some((event, range));
        }
        if searched && self.run.last().is_none_or(|(_, r)| r.end == range.start) {
            self.run.push((event, range));
            return None;
        }
        self.flush();
        if searched {
            self.run.push((event, range));
            return None;
        }
        match &event {
            Event::Start(Tag::CodeBlock(_)) => self.code += 1,
            Event::End(TagEnd::CodeBlock) => self.code -= 1,
            Event::Start(Tag::Link { .. } | Tag::Image { .. }) => self.links += 1,
            Event::End(TagEnd::Link | TagEnd::Image) => self.links -= 1,
            _ => {}
        }
        if self.ready.is_empty() {
            return Some((event, range));
        }
        self.ready.push_back((event, range));
        None
    }

    /// Hands on the run of text, split around the extended autolinks in it.
    fn flush(&mut self) {
        let (Some((_, first)), Some((_, last))) = (self.run.first(), self.run.last()) else {
            return;
        };
        let text = self.text;
        let (start, end) = (first.start, last.end);
        let slice = &text[start..end];
        // Most text holds no link at all: a cheap look before the scan.
        let found = if maybe_link(slice) {
            autolink::find(slice)
        } else {
            Vec::new()
        };
        if found.is_empty() {
            self.ready.extend(self.run.drain(..));
            return;
        }
        self.run.clear();
        let mut at = start;
        for link in found {
            let range = start + link.range.start..start + link.range.end;
            self.plain(at..range.start);
            let link_type = if link.email {
                LinkType::Email
            } else {
                LinkType::Autolink
            };
            self.ready.extend([
                (
                    Event::Start(Tag::Link {
                        link_type,
                        dest_url: link.dest.into(),
                        title: CowStr::Borrowed(""),
                        id: CowStr::Borrowed(""),
                    }),
                    range.clone(),
                ),
                (
                    Event::Text(CowStr::Borrowed(&text[range.clone()])),
                    range.clone(),
                ),
                (Event::End(TagEnd::Link), range.clone()),
            ]);
            at = range.end;
        }
        self.plain(at..end);
    }

    fn plain(&mut self, range: Range<usize>) {
        if !range.is_empty() {
            let text = self.text;
            self.ready
                .push_back((Event::Text(CowStr::Borrowed(&text[range.clone()])), range));
        }
    }
}

/// Whether `text` could hold an extended autolink: a `www.`, a `://` or
/// an `@` (the scan itself is in `autolink.rs`).
fn maybe_link(text: &str) -> bool {
    text.contains("www.") || text.contains("://") || text.contains('@')
}

/// Whether pulldown-cmark's `parsed` text is the source `source` as it
/// stands (not an escape or entity): the same slice, or equal text.
fn is_source(parsed: &str, source: &str) -> bool {
    std::ptr::eq(parsed, source) || parsed == source
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
    fn every_commonmark_example_parses_like_the_spec_but_bare_urls() {
        // GFM's extended autolinks link what these three CommonMark examples
        // say is plain text (comrak with GFM on differs the same way).
        assert_eq!(
            failures("commonmark-0.31.2.json"),
            ["608 Autolinks", "611 Autolinks", "612 Autolinks"]
        );
    }

    #[test]
    fn every_gfm_extension_example_parses_like_the_spec() {
        assert_eq!(failures("gfm-0.29-extensions.json"), Vec::<String>::new());
    }
}
