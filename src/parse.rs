//! Markdown parsing: CommonMark 0.31.2 plus GFM tables, strikethrough, task
//! lists and extended autolinks, through pulldown-cmark (autolinks are
//! livemark's own, `autolink.rs`). The whole document is parsed again on
//! every edit: 0.4 ms for 5,000 lines (PLAN-001 decisions log), so no
//! incremental parser is needed.
mod autolink;

use std::collections::VecDeque;
use std::iter::Peekable;
use std::ops::Range;

use pulldown_cmark::{
    CowStr, Event, LinkType, MetadataBlockKind, OffsetIter, Options, Parser, Tag, TagEnd,
};

/// The syntax pulldown-cmark reads beyond CommonMark; GFM's extended
/// autolinks are added after it.
pub const OPTIONS: Options = Options::ENABLE_TABLES
    .union(Options::ENABLE_STRIKETHROUGH)
    .union(Options::ENABLE_TASKLISTS);

/// The document's parse events with the source range of each: a start
/// or end event spans its whole construct, markers included. A `www.` link,
/// bare URL or email address in text comes as a link of type
/// [`LinkType::Autolink`] or [`LinkType::Email`] whose range has no `<`.
///
/// YAML front matter at the very start (Obsidian's properties) comes as one
/// [`Tag::MetadataBlock`] holding one text event, and the rest is parsed as
/// if it were not there. pulldown-cmark's own option would take a `---`,
/// a line and a `---` anywhere in the document for one.
pub fn events(text: &str) -> impl Iterator<Item = (Event<'_>, Range<usize>)> {
    let front = front_matter(text);
    let meta = front.clone().map(|(block, body)| {
        let kind = MetadataBlockKind::YamlStyle;
        [
            (Event::Start(Tag::MetadataBlock(kind)), block.clone()),
            (Event::Text(CowStr::Borrowed(&text[body.clone()])), body),
            (Event::End(TagEnd::MetadataBlock(kind)), block),
        ]
    });
    let shift = front.map_or(0, |(block, _)| block.end);
    let rest = &text[shift..];
    let events = Events {
        text: rest,
        inner: Parser::new_ext(rest, OPTIONS).into_offset_iter().peekable(),
        ready: VecDeque::new(),
        run: Vec::new(),
        code: 0,
        links: 0,
        text_end: 0,
    };
    meta.into_iter()
        .flatten()
        .chain(events.map(move |(event, range)| (event, range.start + shift..range.end + shift)))
}

/// YAML front matter at the start of `text`: a `---` line, then lines up
/// to a `---` or `...` line, the first of them not blank. The block without its last line ending, and
/// the lines between the fences.
fn front_matter(text: &str) -> Option<(Range<usize>, Range<usize>)> {
    // After a byte order mark too.
    if let Some(rest) = text.strip_prefix('\u{feff}') {
        let bom = text.len() - rest.len();
        return front_matter(rest)
            .map(|(block, body)| (0..block.end + bom, body.start + bom..body.end + bom));
    }
    let fence = |line: &str, closing: bool| {
        let line = line.trim_end_matches([' ', '\t']);
        line == "---" || (closing && line == "...")
    };
    let mut lines = text.split_inclusive(['\n', '\r']).scan(0, |at, line| {
        let start = *at;
        *at += line.len();
        Some((start, line))
    });
    // `\r\n` splits as `\r` then `\n`; the `\n` alone ends nothing new.
    let (_, first) = lines.next()?;
    if !first.ends_with(['\n', '\r']) || !fence(first.trim_end_matches(['\n', '\r']), false) {
        return None;
    }
    let body_start =
        first.len() + usize::from(first.ends_with('\r') && text[first.len()..].starts_with('\n'));
    for (start, line) in lines.filter(|&(start, _)| start >= body_start) {
        if line == "\n" && text[..start].ends_with('\r') {
            continue;
        }
        let content = line.trim_end_matches(['\n', '\r']);
        // Its first line is neither blank nor the end, as in
        // pulldown-cmark: `---` twice is two rules.
        if start == body_start && content.trim().is_empty() {
            return None;
        }
        if fence(content, true) {
            return (start > body_start).then_some((0..start + content.len(), body_start..start));
        }
    }
    None
}

/// The lines of the YAML front matter at the start of `text`, between its
/// fences, if it has one (as [`events`] reads it).
pub fn front_matter_text(text: &str) -> Option<&str> {
    front_matter(text).map(|(_, body)| &text[body])
}

/// Inline `#tags`, Obsidian's: a `#` at a line's start or after a space,
/// then letters, digits, `_`, `-` and `/`, not all of them digits (`#12`
/// is an issue number). Only in text: not in code, front matter or links
/// (a URL's `#fragment` is no tag), and not escaped. Each range covers the
/// `#` and the name.
pub fn tags(text: &str) -> Vec<Range<usize>> {
    let mut found: Vec<Range<usize>> = Vec::new();
    let (mut code, mut links) = (0usize, 0usize);
    for (event, range) in events(text) {
        match event {
            Event::Start(Tag::CodeBlock(_) | Tag::MetadataBlock(_)) => code += 1,
            Event::End(TagEnd::CodeBlock | TagEnd::MetadataBlock(_)) => code -= 1,
            Event::Start(Tag::Link { .. } | Tag::Image { .. }) => links += 1,
            Event::End(TagEnd::Link | TagEnd::Image) => links -= 1,
            Event::Text(_) if code == 0 && links == 0 => {
                let hashes = text[range.clone()].match_indices('#');
                found.extend(hashes.filter_map(|(at, _)| tag_at(text, range.start + at)));
            }
            _ => {}
        }
    }
    found
}

/// The destinations of the links in `text`, as written: inline and
/// reference links, not images, bare URLs or `<...>` autolinks.
pub fn links(text: &str) -> Vec<String> {
    events(text)
        .filter_map(|(event, _)| match event {
            Event::Start(Tag::Link {
                link_type,
                dest_url,
                ..
            }) if !matches!(link_type, LinkType::Autolink | LinkType::Email) => {
                Some(dest_url.into_string())
            }
            _ => None,
        })
        .collect()
}

/// The tag whose `#` is at `start`, by [`tags`]' rule, when the text
/// there is one (code, links and front matter are the caller's to rule
/// out). The name may run on past a text event: pulldown-cmark splits
/// text at `_` and the like.
pub fn tag_at(text: &str, start: usize) -> Option<Range<usize>> {
    let name = |c: char| c.is_alphanumeric() || matches!(c, '_' | '-' | '/');
    if !text.is_char_boundary(start) || !text[start..].starts_with('#') {
        return None;
    }
    let before = text[..start].chars().next_back();
    if before.is_some_and(|c| !c.is_whitespace()) {
        return None;
    }
    let rest = &text[start + 1..];
    let len = rest.find(|c: char| !name(c)).unwrap_or(rest.len());
    let tag = &rest[..len];
    (!tag.is_empty() && !tag.chars().all(|c| c.is_ascii_digit())).then(|| start..start + 1 + len)
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
}
