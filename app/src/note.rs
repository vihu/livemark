//! The vault's note convention (PLAN-004 answers 3 and 6), one place for
//! the app's New note and the command line: `YYYY-MM-DD-slug.md` in the
//! vault's folder, the date the note was made, the slug its title in
//! lowercase ASCII with hyphens; front matter with its title, tags, date
//! and, from an agent, who wrote it. A file is never overwritten: a
//! second note of the same name and day gets `-2`.
use std::io::Write as _;
use std::path::{Path, PathBuf};

/// The longest slug, in bytes, cut at a hyphen.
const SLUG_MAX: usize = 60;

/// What a new note says about itself.
#[derive(Debug, Default)]
pub struct Header<'a> {
    pub title: &'a str,
    pub tags: &'a [String],
    /// `YYYY-MM-DD`.
    pub created: &'a str,
    /// The agent that wrote it, if one did.
    pub by: Option<&'a str>,
}

/// Today's date where the user is, `YYYY-MM-DD`.
pub fn today() -> String {
    jiff::Zoned::now().date().to_string()
}

/// `title` as a file name's slug: accents folded, lowercase ASCII letters
/// and digits, anything else one hyphen; at most 60 bytes; `note` when
/// nothing is left.
pub fn slug(title: &str) -> String {
    let mut slug = String::new();
    for c in title.chars().flat_map(char::to_lowercase) {
        let folded = fold(c);
        if folded.chars().all(|c| c.is_ascii_alphanumeric()) && !folded.is_empty() {
            slug.push_str(folded);
        } else if c.is_ascii_alphanumeric() {
            slug.push(c);
        } else if !slug.ends_with('-') && !slug.is_empty() {
            slug.push('-');
        }
    }
    let mut slug = slug.trim_end_matches('-').to_owned();
    if slug.len() > SLUG_MAX {
        let cut = slug[..SLUG_MAX].rfind('-').unwrap_or(SLUG_MAX);
        slug.truncate(cut);
    }
    if slug.is_empty() { "note".into() } else { slug }
}

/// A lowercase letter with its accent taken off, or `""` for anything not
/// a Latin letter with one.
fn fold(c: char) -> &'static str {
    match c {
        'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'ā' | 'ă' | 'ą' => "a",
        'æ' => "ae",
        'ç' | 'ć' | 'č' => "c",
        'ď' | 'đ' | 'ð' => "d",
        'è' | 'é' | 'ê' | 'ë' | 'ē' | 'ė' | 'ę' | 'ě' => "e",
        'ğ' => "g",
        'ì' | 'í' | 'î' | 'ï' | 'ī' | 'į' | 'ı' => "i",
        'ł' | 'ľ' | 'ĺ' => "l",
        'ñ' | 'ń' | 'ň' => "n",
        'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø' | 'ō' | 'ő' => "o",
        'œ' => "oe",
        'ř' | 'ŕ' => "r",
        'ś' | 'š' | 'ş' | 'ș' => "s",
        'ß' => "ss",
        'ť' | 'ţ' | 'ț' => "t",
        'ù' | 'ú' | 'û' | 'ü' | 'ū' | 'ů' | 'ű' | 'ų' => "u",
        'ý' | 'ÿ' => "y",
        'ź' | 'ż' | 'ž' => "z",
        'þ' => "th",
        _ => "",
    }
}

/// A title as a YAML value: quoted when YAML would read it otherwise.
pub fn yaml_title(title: &str) -> String {
    if title.contains([':', '#', '"', '\''])
        || title.starts_with(['-', '[', '{', '!', '&', '*', '>', '|', '%', '@', '`'])
    {
        format!("\"{}\"", title.replace('\\', "\\\\").replace('"', "\\\""))
    } else {
        title.to_owned()
    }
}

/// The note's front matter, ending with its closing fence's line break.
pub fn front_matter(header: &Header) -> String {
    let title = yaml_title(header.title);
    let mut text = format!(
        "---\ntitle: {title}\ntags: [{}]\ncreated: {}\n",
        header.tags.join(", "),
        header.created
    );
    if let Some(by) = header.by {
        text += &format!("by: {by}\n");
    }
    text + "---\n"
}

/// Writes a new note into `folder`: its front matter, then `body`. Never
/// over an existing file: `-2`, `-3` and on until a name is free.
pub fn create(folder: &Path, header: &Header, body: &str) -> std::io::Result<PathBuf> {
    let base = format!("{}-{}", header.created, slug(header.title));
    let mut text = front_matter(header);
    if !body.is_empty() {
        text.push('\n');
        text.push_str(body);
        if !body.ends_with('\n') {
            text.push('\n');
        }
    }
    for n in 1.. {
        let name = if n == 1 {
            format!("{base}.md")
        } else {
            format!("{base}-{n}.md")
        };
        let path = folder.join(name);
        match std::fs::File::create_new(&path) {
            Ok(mut file) => {
                file.write_all(text.as_bytes())?;
                return Ok(path);
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    }
    unreachable!("some name is free")
}

#[cfg(test)]
mod tests {
    use super::{Header, create, slug, today};

    #[test]
    fn slugs_are_lowercase_ascii_with_hyphens() {
        assert_eq!(slug("Lisbon conference"), "lisbon-conference");
        assert_eq!(slug("  Café: crème brûlée!  "), "cafe-creme-brulee");
        assert_eq!(slug("Straße & Æsir"), "strasse-aesir");
        assert_eq!(slug("Q3 plan (v2)"), "q3-plan-v2");
        assert_eq!(slug("日本"), "note");
        assert_eq!(slug("???"), "note");
        let long = slug(&"word ".repeat(30));
        assert!(long.len() <= 60 && !long.ends_with('-'), "{long}");
    }

    #[test]
    fn dates_come_out_as_the_calendar_has_them() {
        let today = today();
        assert!(today.len() == 10 && today.starts_with("20"), "{today}");
    }

    #[test]
    fn a_note_never_overwrites_another() {
        let dir = std::env::temp_dir().join(format!("livemark-note-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let tags = ["travel".to_owned()];
        let header = Header {
            title: "Lisbon: hotels",
            tags: &tags,
            created: "2026-10-02",
            by: Some("claude-code"),
        };
        let first = create(&dir, &header, "Near Alfama.").unwrap();
        let second = create(&dir, &header, "").unwrap();
        assert!(first.ends_with("2026-10-02-lisbon-hotels.md"));
        assert!(second.ends_with("2026-10-02-lisbon-hotels-2.md"));
        assert_eq!(
            std::fs::read_to_string(&first).unwrap(),
            "---\ntitle: \"Lisbon: hotels\"\ntags: [travel]\ncreated: 2026-10-02\nby: claude-code\n---\n\nNear Alfama.\n"
        );
        // The vault reads it back as written.
        let note = crate::vault::read(
            first.clone(),
            std::time::SystemTime::now(),
            std::fs::read_to_string(&first).unwrap(),
        );
        assert_eq!(
            (note.title.as_str(), note.tags.as_slice()),
            ("Lisbon: hotels", tags.as_slice())
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
