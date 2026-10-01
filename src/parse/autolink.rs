//! GFM's extended autolinks (spec 0.29 section 6.9), which pulldown-cmark
//! does not parse: `www.` links, `http://`, `https://` and `ftp://` URLs,
//! and email addresses in plain text, with the spec's rules for where they
//! end (trailing punctuation, an unmatched `)`, an entity reference, `<`).
//! Edge cases follow cmark-gfm, the spec's reference implementation.
use std::ops::Range;

/// An extended autolink in a stretch of text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Autolink {
    /// Where it is in the text searched.
    pub range: Range<usize>,
    /// Where it goes: `http://` added before `www.`; an email address as
    /// it is (pulldown-cmark's HTML adds `mailto:` for email links).
    pub dest: String,
    pub email: bool,
}

/// The extended autolinks in `text`, in order.
pub(super) fn find(text: &str) -> Vec<Autolink> {
    let bytes = text.as_bytes();
    let mut links: Vec<Autolink> = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        // `www.` and URLs start a word, or follow `*`, `_`, `~` or `(`.
        let boundary = at == 0
            || matches!(
                bytes[at - 1],
                b' ' | b'\t' | b'\n' | b'\r' | b'*' | b'_' | b'~' | b'('
            );
        if boundary {
            if let Some(end) = www(text, at) {
                links.push(Autolink {
                    range: at..end,
                    dest: format!("http://{}", &text[at..end]),
                    email: false,
                });
                at = end;
                continue;
            }
            if let Some(end) = url(text, at) {
                links.push(Autolink {
                    range: at..end,
                    dest: text[at..end].to_owned(),
                    email: false,
                });
                at = end;
                continue;
            }
        }
        if bytes[at] == b'@' {
            let floor = links.last().map_or(0, |l| l.range.end);
            if let Some(range) = email(text, at, floor) {
                at = range.end;
                links.push(Autolink {
                    dest: text[range.clone()].to_owned(),
                    range,
                    email: true,
                });
                continue;
            }
        }
        at += 1;
    }
    links
}

/// A `www.` link at `start`: a valid domain, then anything up to
/// whitespace or `<`, trimmed.
fn www(text: &str, start: usize) -> Option<usize> {
    if !text[start..].starts_with("www.") {
        return None;
    }
    let domain = domain(text, start, false)?;
    Some(start + trim(&text[start..path_end(text, domain)]))
}

/// An `http://`, `https://` or `ftp://` URL at `start`.
fn url(text: &str, start: usize) -> Option<usize> {
    let scheme = ["http://", "https://", "ftp://"]
        .into_iter()
        .find(|s| text[start..].starts_with(s))?;
    let domain = domain(text, start + scheme.len(), true)?;
    Some(start + trim(&text[start..path_end(text, domain)]))
}

/// Where a link's path ends: at whitespace or `<`.
fn path_end(text: &str, from: usize) -> usize {
    text[from..]
        .find(|c: char| c.is_whitespace() || c == '<')
        .map_or(text.len(), |i| from + i)
}

/// The end of a valid domain at `from`: segments of letters, digits, `_`
/// and `-` separated by `.`, no `_` in the last two segments, and a `.`
/// unless `short` (after a URL's scheme).
fn domain(text: &str, from: usize, short: bool) -> Option<usize> {
    let mut end = from;
    let mut dots = 0;
    // Underscores in the segment before the last, and in the last.
    let mut underscores = (0, 0);
    for (i, c) in text[from..].char_indices() {
        match c {
            '.' => {
                underscores = (underscores.1, 0);
                dots += 1;
            }
            '_' => underscores.1 += 1,
            '-' => {}
            c if c.is_alphanumeric() => {}
            _ => break,
        }
        end = from + i + c.len_utf8();
    }
    let valid = end > from && underscores == (0, 0) && (short || dots > 0);
    valid.then_some(end)
}

/// An email address around the `@` at `at`, not reaching back before
/// `floor`: letters, digits, `.`, `+`, `-` and `_` before it; a domain of
/// letters, digits, `-` and `_` with at least one `.` after it, ending in a
/// letter.
fn email(text: &str, at: usize, floor: usize) -> Option<Range<usize>> {
    let bytes = text.as_bytes();
    let mut start = at;
    while start > floor
        && (bytes[start - 1].is_ascii_alphanumeric() || b".+-_".contains(&bytes[start - 1]))
    {
        start -= 1;
    }
    if start == at {
        return None;
    }
    let mut end = at + 1;
    let mut dots = 0;
    while let Some(&b) = bytes.get(end) {
        match b {
            b'.' if bytes.get(end + 1).is_some_and(u8::is_ascii_alphanumeric) => dots += 1,
            b'-' | b'_' => {}
            b if b.is_ascii_alphanumeric() => {}
            _ => break,
        }
        end += 1;
    }
    let last = bytes[end - 1];
    if dots == 0 || bytes.get(end) == Some(&b'@') || !(last.is_ascii_alphabetic() || last == b'.') {
        return None;
    }
    Some(start..start + trim(&text[start..end]))
}

/// How much of `link` stays: up to a `<`; then without trailing `?`, `!`,
/// `.`, `,`, `:`, `*`, `_` or `~`, without a trailing `)` while more close
/// than open, and without a trailing entity reference (`&hl;`).
fn trim(link: &str) -> usize {
    let bytes = link.as_bytes();
    let mut end = bytes.iter().position(|&b| b == b'<').unwrap_or(bytes.len());
    let opening = bytes[..end].iter().filter(|&&b| b == b'(').count();
    let mut closing = bytes[..end].iter().filter(|&&b| b == b')').count();
    while end > 0 {
        match bytes[end - 1] {
            b')' if closing > opening => {
                closing -= 1;
                end -= 1;
            }
            b'?' | b'!' | b'.' | b',' | b':' | b'*' | b'_' | b'~' => end -= 1,
            b';' => {
                let letters = bytes[..end - 1]
                    .iter()
                    .rev()
                    .take_while(|b| b.is_ascii_alphabetic())
                    .count();
                let amp = end - 1 - letters;
                end = if letters > 0 && amp > 0 && bytes[amp - 1] == b'&' {
                    amp - 1
                } else {
                    end - 1
                };
            }
            _ => break,
        }
    }
    end
}

#[cfg(test)]
mod tests {
    use super::find;

    fn links(text: &str) -> Vec<(&str, String)> {
        find(text)
            .into_iter()
            .map(|l| (&text[l.range], l.dest))
            .collect()
    }

    #[test]
    fn www_urls_and_emails_end_where_the_spec_says() {
        assert_eq!(
            links("Visit www.commonmark.org/a.b."),
            [(
                "www.commonmark.org/a.b",
                "http://www.commonmark.org/a.b".into()
            )]
        );
        assert_eq!(
            links("(www.x.y/q=(a)))")[0].0,
            "www.x.y/q=(a)",
            "unmatched `)` go"
        );
        assert_eq!(
            links("www.x.y/q=c&hl;")[0].0,
            "www.x.y/q=c",
            "an entity at the end goes"
        );
        assert_eq!(links("www.x.y/he<lp")[0].0, "www.x.y/he");
        assert_eq!(
            links("see https://é.example/ü!")[0].0,
            "https://é.example/ü"
        );
        assert_eq!(links("a.b-c_d@a.b.")[0].1, "a.b-c_d@a.b");
        assert!(links("a.b-c_d@a.b-").is_empty());
        assert!(links("hello@mail+xyz.example").is_empty());
        assert!(
            links("xwww.example.com and www.x_y.z_w").is_empty(),
            "inside a word; `_` in the last segments"
        );
    }
}
