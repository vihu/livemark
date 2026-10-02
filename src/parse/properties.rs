//! The front matter's properties as live preview draws them (PLAN-005):
//! its `title`, its `created` date and its `tags` list, read by hand as
//! notes write them (`[a, b]`, `a, b`, or a `- a` line each), every item
//! with its place in the source, so a tag can be taken out or put in with
//! nothing else touched.
use std::ops::Range;

/// What the front matter says, with where.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Properties {
    /// The block, fences included, without its last line ending.
    pub block: Range<usize>,
    /// Where the closing fence's line starts.
    pub closing: usize,
    /// The `title` value as written (quotes included), trimmed.
    pub title: Option<Range<usize>>,
    /// The `created` value as written, trimmed.
    pub created: Option<Range<usize>>,
    /// The `tags` (or `tag`) list, when the key is there.
    pub tags: Option<TagList>,
}

/// The `tags` list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TagList {
    /// The tags, in order (an empty one where a `- ` line has none yet).
    pub items: Vec<Item>,
    /// How the list is written.
    pub form: Form,
}

/// One tag in the list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Item {
    /// As written, quotes and `#` included, trimmed.
    pub range: Range<usize>,
    /// The name, quotes and `#` off.
    pub name: String,
    /// What taking it out removes: with a comma and space beside it, or
    /// its whole line.
    pub cut: Range<usize>,
}

/// How the list is written.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Form {
    /// `tags: [a, b]`.
    Flow {
        /// Where the `[` is.
        open: usize,
        /// Where the `]` is (the line's end when missing).
        close: usize,
    },
    /// `tags: a, b`.
    Plain {
        /// The values after the colon, trimmed.
        values: Range<usize>,
    },
    /// `- a` lines under `tags:`.
    Block {
        /// What comes before each dash.
        indent: String,
        /// Where the last item's line ends, before its line ending.
        end: usize,
    },
    /// Nothing after `tags:`.
    Empty {
        /// The space after its colon, to the line's end.
        rest: Range<usize>,
    },
}

/// The front matter's properties, if `text` starts with front matter.
pub fn properties(text: &str) -> Option<Properties> {
    let (block, body) = super::front_matter(text)?;
    let mut found = Properties {
        closing: body.end,
        block,
        title: None,
        created: None,
        tags: None,
    };
    let lines = lines_of(text, body);
    let mut i = 0;
    while i < lines.len() {
        let (start, end, _) = lines[i];
        i += 1;
        let line = &text[start..end];
        // Keys only at the line's start.
        if line.starts_with([' ', '\t', '-', '#']) {
            continue;
        }
        let Some(colon) = line.find(':') else {
            continue;
        };
        let after = start + colon + 1;
        let value = &text[after..end];
        let lead = value.len() - value.trim_start().len();
        let values = after + lead..after + lead + value.trim().len();
        match line[..colon].trim() {
            "title" if !values.is_empty() => found.title = Some(values),
            "created" if !values.is_empty() => found.created = Some(values),
            "tags" | "tag" if values.is_empty() => {
                // `- a` lines, up to the first line that is not one.
                let mut items = Vec::new();
                let mut indent = String::new();
                let mut last = end;
                while let Some(&(start, end, next)) = lines.get(i) {
                    let line = &text[start..end];
                    let item = line.trim_start_matches([' ', '\t']);
                    if !(item.starts_with("- ") || item == "-") {
                        break;
                    }
                    i += 1;
                    if items.is_empty() {
                        indent = line[..line.len() - item.len()].to_owned();
                    }
                    let dash = end - item.len();
                    let value = &text[dash + 1..end];
                    let lead = value.len() - value.trim_start().len();
                    let range = dash + 1 + lead..dash + 1 + lead + value.trim().len();
                    items.push(Item {
                        name: name(&text[range.clone()]),
                        range,
                        cut: start..next,
                    });
                    last = end;
                }
                let form = if items.is_empty() {
                    Form::Empty { rest: after..end }
                } else {
                    Form::Block { indent, end: last }
                };
                found.tags = Some(TagList { items, form });
            }
            "tags" | "tag" => {
                let (form, inside) = if text[values.clone()].starts_with('[') {
                    let open = values.start;
                    let close = text[open..end].rfind(']').map_or(end, |at| open + at);
                    (Form::Flow { open, close }, open + 1..close)
                } else {
                    (
                        Form::Plain {
                            values: values.clone(),
                        },
                        values,
                    )
                };
                found.tags = Some(TagList {
                    items: listed(text, inside),
                    form,
                });
            }
            _ => {}
        }
    }
    Some(found)
}

/// The comma-separated items in `inside`, each with its cut.
fn listed(text: &str, inside: Range<usize>) -> Vec<Item> {
    let mut ranges: Vec<Range<usize>> = Vec::new();
    let mut at = inside.start;
    for part in text[inside].split(',') {
        let lead = part.len() - part.trim_start().len();
        let len = part.trim().len();
        if len > 0 {
            ranges.push(at + lead..at + lead + len);
        }
        at += part.len() + 1;
    }
    (0..ranges.len())
        .map(|i| {
            let range = ranges[i].clone();
            let cut = if i + 1 < ranges.len() {
                // With the comma and spaces up to the next one.
                range.start..ranges[i + 1].start
            } else if i > 0 {
                // The last: from the end of the one before.
                ranges[i - 1].end..range.end
            } else {
                range.clone()
            };
            Item {
                name: name(&text[range.clone()]),
                range,
                cut,
            }
        })
        .collect()
}

/// A list item's name: quotes and `#` off.
fn name(item: &str) -> String {
    item.trim_matches(['"', '\''])
        .trim()
        .trim_start_matches('#')
        .to_owned()
}

/// A value as shown: quotes off, `\"` and `''` in them unescaped.
pub fn unquoted(value: &str) -> String {
    if let Some(inner) = value.strip_prefix('"').and_then(|v| v.strip_suffix('"')) {
        inner.replace("\\\"", "\"").replace("\\\\", "\\")
    } else if let Some(inner) = value.strip_prefix('\'').and_then(|v| v.strip_suffix('\'')) {
        inner.replace("''", "'")
    } else {
        value.to_owned()
    }
}

/// The lines in `range` of `text`: where each starts, where its text ends
/// and where the next starts (`\n`, `\r\n` or `\r` endings).
fn lines_of(text: &str, range: Range<usize>) -> Vec<(usize, usize, usize)> {
    let mut lines = Vec::new();
    let mut at = range.start;
    while at < range.end {
        let rest = &text[at..range.end];
        let end = at + rest.find(['\n', '\r']).unwrap_or(rest.len());
        let next = if text[end..].starts_with("\r\n") {
            end + 2
        } else {
            (end + 1).min(text.len())
        };
        lines.push((at, end, next));
        at = next.max(at + 1);
    }
    lines
}

/// The tag being typed in the front matter's `tags` list with the caret
/// at `offset`: from where the item starts up to the caret (empty right
/// after a `[`, a comma or a `- `), when only a tag's characters are
/// between.
pub fn tag_item_at(properties: &Properties, text: &str, offset: usize) -> Option<Range<usize>> {
    let list = properties.tags.as_ref()?;
    let line_start = text[..offset].rfind(['\n', '\r']).map_or(0, |i| i + 1);
    let zone = match &list.form {
        Form::Flow { open, close } => *open + 1..*close,
        Form::Plain { values } => values.clone(),
        Form::Empty { rest } => rest.clone(),
        // On an item's line, after its dash.
        Form::Block { .. } => {
            let item = list
                .items
                .iter()
                .find(|item| item.cut.start == line_start)?;
            let line = text[item.cut.clone()].trim_end_matches(['\n', '\r']);
            let dash = line_start + line.find('-')? + 1;
            dash..line_start + line.len()
        }
    };
    if !(zone.start <= offset && offset <= zone.end) || line_start > zone.start {
        return None;
    }
    let before = &text[zone.start..offset];
    let start = zone.start + before.rfind(',').map_or(0, |i| i + 1);
    let typed = &text[start..offset];
    let start = start + typed.len() - typed.trim_start().len();
    let name = |c: char| c.is_alphanumeric() || matches!(c, '_' | '-' | '/');
    text[start..offset]
        .chars()
        .all(name)
        .then_some(start..offset)
}

#[cfg(test)]
mod tests {
    use super::{Form, properties, tag_item_at, unquoted};

    fn names(text: &str) -> Vec<String> {
        let tags = properties(text).unwrap().tags.unwrap();
        tags.items.into_iter().map(|item| item.name).collect()
    }

    #[test]
    fn the_title_date_and_tags_are_found_in_every_list_form() {
        let text = "---\ntitle: \"Trip: Lisbon\"\ntags: [work, \"#travel\"]\ncreated: 2026-10-02\nby: agent\n---\nbody\n";
        let found = properties(text).unwrap();
        assert_eq!(&text[found.title.clone().unwrap()], "\"Trip: Lisbon\"");
        assert_eq!(unquoted(&text[found.title.unwrap()]), "Trip: Lisbon");
        assert_eq!(&text[found.created.unwrap()], "2026-10-02");
        assert_eq!(&text[found.closing..found.block.end], "---");
        assert_eq!(names(text), ["work", "travel"]);
        assert_eq!(names("---\ntags: a, b\n---\n"), ["a", "b"]);
        assert_eq!(
            names("---\r\ntags:\r\n  - a\r\n  - b\r\nx: 1\r\n---\r\n"),
            ["a", "b"]
        );
        let block = properties("---\ntags:\n  - a\n  - b\n---\n")
            .unwrap()
            .tags
            .unwrap();
        assert_eq!(
            block.form,
            Form::Block {
                indent: "  ".into(),
                end: 21
            }
        );
        assert_eq!(block.items[1].cut, 16..22, "the whole line goes");
        let empty = properties("---\ntags:\n---\n").unwrap().tags.unwrap();
        assert!(empty.items.is_empty() && matches!(empty.form, Form::Empty { .. }));
        assert_eq!(properties("no front matter"), None);
        assert_eq!(properties("---\ntitle: x\n---\n").unwrap().tags, None);
    }

    #[test]
    fn cuts_take_an_item_with_one_comma() {
        let text = "---\ntags: [a, b, c]\n---\n";
        let items = properties(text).unwrap().tags.unwrap().items;
        let without = |i: usize| {
            let cut = items[i].cut.clone();
            format!("{}{}", &text[..cut.start], &text[cut.end..])
        };
        assert_eq!(without(0), "---\ntags: [b, c]\n---\n");
        assert_eq!(without(1), "---\ntags: [a, c]\n---\n");
        assert_eq!(without(2), "---\ntags: [a, b]\n---\n");
    }

    #[test]
    fn the_tag_being_typed_is_found_only_inside_the_list() {
        let at = |text: &str, offset: usize| {
            let found = properties(text).unwrap();
            tag_item_at(&found, text, offset).map(|r| text[r].to_owned())
        };
        let flow = "---\ntags: [work, tr]\n---\n";
        assert_eq!(at(flow, 19), Some("tr".into()));
        assert_eq!(at(flow, 11), Some(String::new()), "right after the [");
        assert_eq!(at(flow, 8), None, "in the key");
        let block = "---\ntags:\n  - wo\n---\n";
        assert_eq!(at(block, 16), Some("wo".into()));
        assert_eq!(at("---\ntags: \n---\n", 10), Some(String::new()));
        assert_eq!(at("---\ntitle: x\ntags: [a]\n---\n", 12), None);
    }
}
