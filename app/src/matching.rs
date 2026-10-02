//! How the search field matches, and what it marks (PLAN-006): a title by
//! its words, in any order, each query word the start of a title word,
//! inside one (three letters or more), or a typo away from one (a letter
//! added, dropped, changed or two swapped, from four letters; two from
//! eight); text by every word as written, any case. `occurrences` gives
//! where words sit in a line, to mark them.
use std::ops::Range;

/// How well `query` matches `title`, and the title's stretches that
/// matched; `None` when a query word is in none of its words. Words that
/// match outright count most, then starts, then insides, then typos; the
/// title's own first word and words in its order a little more. An empty
/// query matches every title equally.
pub fn title_match(title: &str, query: &str) -> Option<(i32, Vec<Range<usize>>)> {
    let words = words_of(title);
    let mut score = 0;
    let mut marks: Vec<Range<usize>> = Vec::new();
    let mut last = None;
    for wanted in query.split_whitespace().map(str::to_lowercase) {
        let wanted: Vec<char> = wanted.chars().collect();
        let best = words
            .iter()
            .enumerate()
            .filter_map(|(i, (range, word))| {
                word_match(&wanted, word, &title[range.clone()]).map(|(points, within)| {
                    (
                        points,
                        i,
                        range.start + within.start..range.start + within.end,
                    )
                })
            })
            .max_by(|a, b| a.0.cmp(&b.0).then(b.1.cmp(&a.1)))?;
        let (points, index, range) = best;
        score += points;
        if index == 0 {
            score += 5;
        }
        if last.is_some_and(|last| index > last) {
            score += 3;
        }
        last = Some(index);
        marks.push(range);
    }
    marks.sort_by_key(|r| r.start);
    marks.dedup();
    Some((score, marks))
}

/// One query word against one title word (lowercase `word`, `original` as
/// written): its points and the stretch of `original` it covers.
fn word_match(wanted: &[char], word: &[char], original: &str) -> Option<(i32, Range<usize>)> {
    // Byte offsets of the original word's characters, to mark them.
    let at: Vec<usize> = original
        .char_indices()
        .map(|(i, _)| i)
        .chain([original.len()])
        .collect();
    let span = |from: usize, to: usize| at[from.min(at.len() - 1)]..at[to.min(at.len() - 1)];
    let (n, len) = (wanted.len(), word.len());
    if word == wanted {
        return Some((100, span(0, len)));
    }
    if word.starts_with(wanted) {
        return Some((80, span(0, n)));
    }
    if n >= 3
        && let Some(from) = (1..=len.saturating_sub(n)).find(|&i| word[i..i + n] == *wanted)
    {
        return Some((50, span(from, from + n)));
    }
    let most = match n {
        0..=3 => return None,
        4..=7 => 1,
        _ => 2,
    };
    // The whole word, or its start as far as typed.
    if near(wanted, word, most) {
        return Some((40, span(0, len)));
    }
    if len > n && near(wanted, &word[..n], most) {
        return Some((30, span(0, n)));
    }
    None
}

/// A title's words (letters and digits) with their byte ranges, lowercase.
fn words_of(title: &str) -> Vec<(Range<usize>, Vec<char>)> {
    let mut words = Vec::new();
    let mut start = None;
    for (i, c) in title.char_indices().chain([(title.len(), ' ')]) {
        match (c.is_alphanumeric(), start) {
            (true, None) => start = Some(i),
            (false, Some(from)) => {
                let word = title[from..i].to_lowercase().chars().collect();
                words.push((from..i, word));
                start = None;
            }
            _ => {}
        }
    }
    words
}

/// Whether `a` and `b` are at most `most` typos apart: a letter added,
/// dropped or changed, or two next to each other swapped.
fn near(a: &[char], b: &[char], most: usize) -> bool {
    if a.len().abs_diff(b.len()) > most {
        return false;
    }
    let (n, m) = (a.len(), b.len());
    // Row 0 counts insertions, column 0 deletions.
    let mut d: Vec<Vec<usize>> = (0..=n)
        .map(|i| {
            (0..=m)
                .map(|j| {
                    if i == 0 {
                        j
                    } else if j == 0 {
                        i
                    } else {
                        0
                    }
                })
                .collect()
        })
        .collect();
    for i in 1..=n {
        for j in 1..=m {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            let mut best = (d[i - 1][j] + 1)
                .min(d[i][j - 1] + 1)
                .min(d[i - 1][j - 1] + cost);
            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                best = best.min(d[i - 2][j - 2] + 1);
            }
            d[i][j] = best;
        }
    }
    d[n][m] <= most
}

/// Where any of `words` (lowercase) sits in `text`, any case: byte ranges
/// of `text`, sorted, joined where they touch.
pub fn occurrences(text: &str, words: &[String]) -> Vec<Range<usize>> {
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let lower: Vec<char> = chars
        .iter()
        .map(|&(_, c)| c.to_lowercase().next().unwrap_or(c))
        .collect();
    let end = |i: usize| chars.get(i).map_or(text.len(), |&(at, _)| at);
    let mut found: Vec<Range<usize>> = Vec::new();
    for word in words.iter().filter(|w| !w.is_empty()) {
        let word: Vec<char> = word.chars().collect();
        let n = word.len();
        let mut i = 0;
        while i + n <= lower.len() {
            if lower[i..i + n] == word[..] {
                found.push(end(i)..end(i + n));
                i += n;
            } else {
                i += 1;
            }
        }
    }
    found.sort_by_key(|r| r.start);
    let mut joined: Vec<Range<usize>> = Vec::new();
    for range in found {
        match joined.last_mut() {
            Some(last) if range.start <= last.end => last.end = last.end.max(range.end),
            _ => joined.push(range),
        }
    }
    joined
}

#[cfg(test)]
mod tests {
    use super::{occurrences, title_match};

    fn marked(title: &str, query: &str) -> Option<Vec<String>> {
        title_match(title, query)
            .map(|(_, marks)| marks.into_iter().map(|r| title[r].to_owned()).collect())
    }

    #[test]
    fn titles_match_by_words_in_any_order_forgiving_a_typo() {
        let title = "Secure Boot bring-up on the Utah bare-metal server";
        // Letters scattered through the title are no match.
        assert_eq!(marked(title, "travel"), None);
        assert_eq!(marked(title, "boot secure").unwrap(), ["Secure", "Boot"]);
        assert_eq!(marked(title, "ser").unwrap(), ["ser"], "a start");
        assert_eq!(marked(title, "metal").unwrap(), ["metal"]);
        assert_eq!(marked(title, "ecur").unwrap(), ["ecur"], "inside");
        assert_eq!(marked(title, "sevrer").unwrap(), ["server"], "two swapped");
        assert_eq!(marked(title, "bootz").unwrap(), ["Boot"], "one added");
        assert_eq!(marked(title, "kickloadr"), None);
        assert_eq!(
            marked("Kickloader notes", "kicklaod").unwrap(),
            ["Kickload"]
        );
        // Short words are matched only as written.
        assert_eq!(marked(title, "bot"), None);
        assert_eq!(marked(title, ""), Some(Vec::new()));
        // Outright beats a start, a start beats a typo.
        let score = |t: &str, q: &str| title_match(t, q).unwrap().0;
        assert!(score("Lisbon hotels", "hotels") > score("Hotelsmith", "hotels"));
        assert!(score("Standup", "stand") > score("Standing", "stamd"));
    }

    #[test]
    fn occurrences_are_marked_in_any_case_and_joined() {
        let text = "The Hotel near hotels, ÉTÉ été";
        let found: Vec<&str> = occurrences(text, &["hotel".into(), "été".into()])
            .into_iter()
            .map(|r| &text[r])
            .collect();
        assert_eq!(found, ["Hotel", "hotel", "ÉTÉ", "été"]);
    }
}
