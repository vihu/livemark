//! A note renamed (PLAN-006): its title set where the note keeps it, the
//! file name it gets (its date kept), and the links in other notes that
//! follow it. Plain text in, text out: `note_actions.rs` writes them.
use std::ops::Range;
use std::path::{Path, PathBuf};

use super::links::relative;
use super::vault::resolve;

/// `text` with its title set to `new`: the front matter's `title`, or a
/// line for it, or the first `#` heading when that was the title (`old`);
/// `None` when the title is the file's name.
pub(crate) fn retitled(text: &str, old: &str, new: &str) -> Option<String> {
    let value = super::note::yaml_title(new);
    if let Some(properties) = livemark::parse::properties::properties(text) {
        if let Some(range) = properties.title {
            return Some(splice(text, &[(range, value)]));
        }
        // After the opening fence's line.
        let at = text.find('\n').map_or(text.len(), |i| i + 1);
        let nl = if text[..at].ends_with("\r\n") {
            "\r\n"
        } else {
            "\n"
        };
        return Some(splice(text, &[(at..at, format!("title: {value}{nl}"))]));
    }
    let mut start = 0;
    for line in text.split_inclusive('\n') {
        let content = line.trim_end_matches(['\n', '\r']);
        if let Some(heading) = content.strip_prefix("# ")
            && heading.trim() == old
        {
            let from = start + 2 + (heading.len() - heading.trim_start().len());
            let to = from + heading.trim().len();
            return Some(splice(text, &[(from..to, new.to_owned())]));
        }
        start += line.len();
    }
    None
}

/// The file the note at `path` gets when titled `title`: its date kept,
/// the title's slug, `-2`, `-3` when taken; `path` itself when that is it.
pub(crate) fn free_path(path: &Path, title: &str) -> PathBuf {
    let folder = path.parent().unwrap_or(Path::new("."));
    let stem = path
        .file_stem()
        .map_or_else(String::new, |s| s.to_string_lossy().into_owned());
    let dated = stem.len() > 11
        && stem.as_bytes()[10] == b'-'
        && stem[..10].bytes().enumerate().all(|(i, b)| {
            if i == 4 || i == 7 {
                b == b'-'
            } else {
                b.is_ascii_digit()
            }
        });
    let base = if dated {
        format!("{}{}", &stem[..11], super::note::slug(title))
    } else {
        super::note::slug(title)
    };
    (1..)
        .map(|n| match n {
            1 => folder.join(format!("{base}.md")),
            n => folder.join(format!("{base}-{n}.md")),
        })
        .find(|candidate| candidate == path || !candidate.exists())
        .unwrap_or_else(|| path.to_path_buf())
}

/// `text` (the note at `from`) with its links to `old` pointing at `new`,
/// `#part`s kept, and how many changed; `None` when none did.
pub(crate) fn relinked(text: &str, from: &Path, old: &Path, new: &Path) -> Option<(String, usize)> {
    let to = |dest: &str| {
        let part = dest.find('#').map_or("", |i| &dest[i..]);
        format!("{}{part}", relative(from, new))
    };
    let mut changes: Vec<(Range<usize>, String)> = Vec::new();
    for (range, dest) in livemark::parse::link_spans(text) {
        if resolve(from, &dest).as_deref() != Some(old) {
            continue;
        }
        // An inline link: its destination as written after `](`.
        let link = &text[range.clone()];
        let Some(at) = link.rfind("](") else {
            continue;
        };
        let start = range.start + at + 2;
        let written = &text[start..range.end];
        let (start, len) = match written.strip_prefix('<') {
            Some(rest) => (start + 1, rest.find('>').unwrap_or(rest.len())),
            None => (
                start,
                written
                    .find(|c: char| c.is_whitespace() || c == ')')
                    .unwrap_or(written.len()),
            ),
        };
        changes.push((start..start + len, to(&dest)));
    }
    // Reference definitions: `[label]: destination`.
    let mut at = 0;
    for line in text.split_inclusive('\n') {
        let content = line.trim_end_matches(['\n', '\r']);
        let trimmed = content.trim_start();
        if trimmed.starts_with('[')
            && let Some(close) = trimmed.find("]:")
        {
            let rest = &trimmed[close + 2..];
            let dest = rest.trim_start();
            let dest = dest.split_whitespace().next().unwrap_or("");
            let dest = dest.trim_start_matches('<').trim_end_matches('>');
            if !dest.is_empty() && resolve(from, dest).as_deref() == Some(old) {
                let start = at + content.find(dest).unwrap_or(0);
                changes.push((start..start + dest.len(), to(dest)));
            }
        }
        at += line.len();
    }
    changes.sort_by_key(|(r, _)| r.start);
    changes.dedup_by(|a, b| a.0 == b.0);
    let count = changes.len();
    (count > 0).then(|| (splice(text, &changes), count))
}

/// `text` with each range replaced; the ranges sorted, apart.
fn splice(text: &str, changes: &[(Range<usize>, String)]) -> String {
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    for (range, with) in changes {
        out.push_str(&text[at..range.start]);
        out.push_str(with);
        at = range.end;
    }
    out.push_str(&text[at..]);
    out
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{relinked, retitled};

    #[test]
    fn a_title_is_set_where_the_note_keeps_it() {
        assert_eq!(
            retitled("---\ntitle: Old\ntags: []\n---\nbody\n", "Old", "New: one").unwrap(),
            "---\ntitle: \"New: one\"\ntags: []\n---\nbody\n"
        );
        assert_eq!(
            retitled("---\r\ntags: []\r\n---\r\n", "x", "New").unwrap(),
            "---\r\ntitle: New\r\ntags: []\r\n---\r\n"
        );
        assert_eq!(
            retitled("# Old\nbody\n", "Old", "New").unwrap(),
            "# New\nbody\n"
        );
        assert_eq!(retitled("just text\n", "file-name", "New"), None);
    }

    #[test]
    fn links_to_a_renamed_note_follow_it_with_their_parts() {
        let from = Path::new("/v/sub/a.md");
        let (old, new) = (Path::new("/v/b.md"), Path::new("/v/b-new.md"));
        let text = "See [b](../b.md#top) and <[c](c.md)>, [ref][r].\n\n[r]: ../b.md\n";
        let (text, n) = relinked(text, from, old, new).unwrap();
        assert_eq!(
            text,
            "See [b](../b-new.md#top) and <[c](c.md)>, [ref][r].\n\n[r]: ../b-new.md\n"
        );
        assert_eq!(n, 2);
        assert_eq!(relinked("no links\n", from, old, new), None);
    }
}
