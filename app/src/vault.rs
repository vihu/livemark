//! A vault (PLAN-004): a folder of markdown notes, read into memory, no
//! database and no files of the app's own in it. Each note's title, tags
//! and creation date come from its front matter (`title`, `tags`,
//! `created`), else its first heading and its file name's date; inline
//! `#tags` count too. Read again by modification time when asked
//! (`refresh`), so changes from git, other editors or agents show up.
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use livemark::parse::properties::{properties, unquoted};

/// A note as the vault knows it.
#[derive(Debug, Clone)]
pub struct Note {
    pub path: PathBuf,
    pub title: String,
    /// Lowercase, each once, in order of first use.
    pub tags: Vec<String>,
    /// `YYYY-MM-DD`, from front matter or the file name.
    pub created: Option<String>,
    pub modified: SystemTime,
    /// The file's size: with the time, what tells a changed file.
    pub size: u64,
    /// The whole text, and in lowercase, for search.
    pub text: String,
    pub lower: String,
    /// The notes it links to, as files (for "Linked from").
    pub links: Vec<PathBuf>,
}

#[derive(Debug)]
pub struct Vault {
    pub root: PathBuf,
    /// Most recently modified first.
    pub notes: Vec<Note>,
    /// Changes whenever a note comes, goes or changes: what a view of the
    /// notes is cached by.
    pub generation: u64,
    /// When the git repository the vault is in last moved its HEAD (a
    /// commit, a pull), read from `.git/logs/HEAD`; `None` outside one.
    pub last_commit: Option<SystemTime>,
}

impl Vault {
    /// The vault at `root`, every note read.
    pub fn open(root: &Path) -> std::io::Result<Self> {
        if !root.is_dir() {
            return Err(std::io::Error::other(format!(
                "{}: not a folder",
                root.display()
            )));
        }
        let mut vault = Self {
            // As recent files are kept: one path per note.
            root: std::fs::canonicalize(root)?,
            notes: Vec::new(),
            generation: 0,
            last_commit: None,
        };
        vault.refresh();
        Ok(vault)
    }

    /// Reads again the notes whose files changed, came or went; returns
    /// whether anything did.
    pub fn refresh(&mut self) -> bool {
        let files = walk(&self.root);
        let mut changed = files.len() != self.notes.len();
        let mut old: BTreeMap<PathBuf, Note> = std::mem::take(&mut self.notes)
            .into_iter()
            .map(|note| (note.path.clone(), note))
            .collect();
        for (path, modified, size) in files {
            match old.remove(&path) {
                Some(note) if (note.modified, note.size) == (modified, size) => {
                    self.notes.push(note);
                }
                _ => {
                    changed = true;
                    if let Ok(text) = std::fs::read_to_string(&path) {
                        self.notes.push(read(path, modified, text));
                    }
                }
            }
        }
        // Then newest created, then by title: a fresh clone gives every
        // file the same time.
        self.notes.sort_by(|a, b| {
            (b.modified, &b.created)
                .cmp(&(a.modified, &a.created))
                .then_with(|| a.title.cmp(&b.title))
        });
        let last_commit = last_commit(&self.root);
        changed |= last_commit != self.last_commit;
        self.last_commit = last_commit;
        if changed {
            self.generation += 1;
        }
        changed
    }

    /// How many notes changed since the last commit, going by file times
    /// (the app never runs git, PLAN-004 answer 4): `None` outside a git
    /// repository. Deleted notes are not counted.
    pub fn uncommitted(&self) -> Option<usize> {
        let since = self.last_commit?;
        Some(
            self.notes
                .iter()
                .filter(|note| note.modified > since)
                .count(),
        )
    }

    /// Every tag with how many notes have it, most used first, then by
    /// name.
    pub fn tags(&self) -> Vec<(String, usize)> {
        let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
        for tag in self.notes.iter().flat_map(|note| &note.tags) {
            *counts.entry(tag).or_default() += 1;
        }
        let mut tags: Vec<(String, usize)> = counts
            .into_iter()
            .map(|(tag, count)| (tag.to_owned(), count))
            .collect();
        tags.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        tags
    }

    /// The notes linking to the note at `path`, most recent first.
    pub fn linked_from(&self, path: &Path) -> Vec<&Note> {
        self.notes
            .iter()
            .filter(|note| note.path != path && note.links.iter().any(|l| l == path))
            .collect()
    }

    /// The folder's name, for the sidebar.
    pub fn name(&self) -> String {
        self.root.file_name().map_or_else(
            || self.root.display().to_string(),
            |name| name.to_string_lossy().into_owned(),
        )
    }
}

/// When HEAD last moved in the repository holding `root`: the time on the
/// last line of `.git/logs/HEAD` (`<old> <new> <name> <<email>> <seconds>
/// <zone>\t<message>`), looking up from `root` for `.git`.
fn last_commit(root: &Path) -> Option<SystemTime> {
    let git = root
        .ancestors()
        .map(|dir| dir.join(".git"))
        .find(|git| git.exists())?;
    // A worktree's or submodule's `.git` file points at its git folder.
    let git = if git.is_file() {
        let pointer = std::fs::read_to_string(&git).ok()?;
        let dir = pointer.trim().strip_prefix("gitdir:")?.trim();
        git.parent()?.join(dir)
    } else {
        git
    };
    let log = std::fs::read_to_string(git.join("logs").join("HEAD")).ok()?;
    let line = log.lines().last()?;
    let head = line.split('\t').next()?;
    let seconds: u64 = head.rsplit(' ').nth(1)?.parse().ok()?;
    Some(SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(seconds))
}

/// The markdown files under `dir` with their modification times and
/// sizes; names starting with a dot (`.git`, `.obsidian`) are left out.
/// Symbolic links are followed (a loop is skipped), so a linked note's own
/// time is the one compared.
fn walk(dir: &Path) -> Vec<(PathBuf, SystemTime, u64)> {
    let hidden = |entry: &walkdir::DirEntry| {
        entry.depth() > 0 && entry.file_name().to_string_lossy().starts_with('.')
    };
    walkdir::WalkDir::new(dir)
        .follow_links(true)
        .into_iter()
        .filter_entry(|entry| !hidden(entry))
        .flatten()
        .filter(|entry| {
            entry.file_type().is_file()
                && entry
                    .path()
                    .extension()
                    .is_some_and(|e| e == "md" || e == "markdown")
        })
        .filter_map(|entry| {
            let metadata = entry.metadata().ok()?;
            let modified = metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH);
            Some((entry.into_path(), modified, metadata.len()))
        })
        .collect()
}

/// A note from its file's text.
pub fn read(path: PathBuf, modified: SystemTime, text: String) -> Note {
    let (title, mut tags, created) = front_matter(&text);
    for range in livemark::parse::tags(&text) {
        let tag = text[range.start + 1..range.end].to_lowercase();
        if !tags.contains(&tag) {
            tags.push(tag);
        }
    }
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let title = title
        .or_else(|| first_heading(&text))
        .unwrap_or_else(|| stem.clone());
    let created = created.or_else(|| date_prefix(&stem));
    let links = livemark::parse::links(&text)
        .iter()
        .filter_map(|dest| resolve(&path, dest))
        .collect();
    Note {
        path,
        title,
        tags,
        created,
        modified,
        size: text.len() as u64,
        lower: text.to_lowercase(),
        links,
        text,
    }
}

/// The markdown file a link in the note at `from` points to: relative to
/// the note's folder, `%20` and the like decoded, any `#part` dropped;
/// `None` for web and mail links and links to other files. The file need
/// not exist.
pub fn resolve(from: &Path, dest: &str) -> Option<PathBuf> {
    let dest = dest.split(['#', '?']).next()?;
    if dest.is_empty() || dest.contains("://") || dest.starts_with("mailto:") {
        return None;
    }
    let decoded = percent_encoding::percent_decode_str(dest).decode_utf8_lossy();
    let target = Path::new(decoded.as_ref());
    if !target
        .extension()
        .is_some_and(|e| e == "md" || e == "markdown")
    {
        return None;
    }
    let joined = from.parent().unwrap_or(Path::new("")).join(target);
    // `..` and `.` taken out by the names alone: the file may not exist.
    let mut clean = PathBuf::new();
    for part in joined.components() {
        match part {
            std::path::Component::ParentDir => {
                clean.pop();
            }
            std::path::Component::CurDir => {}
            other => clean.push(other),
        }
    }
    Some(clean)
}

/// `title`, `tags` and `created` from the front matter, read as the
/// editor reads them (`livemark::parse::properties`), so a note never
/// fails to load over it.
fn front_matter(text: &str) -> (Option<String>, Vec<String>, Option<String>) {
    let Some(front) = properties(text) else {
        return Default::default();
    };
    let value = |range: Option<std::ops::Range<usize>>| range.map(|r| unquoted(&text[r]));
    let title = value(front.title).filter(|title| !title.is_empty());
    let created = value(front.created).and_then(|created| date_prefix(&created));
    let mut tags: Vec<String> = Vec::new();
    for item in front.tags.iter().flat_map(|list| &list.items) {
        let tag = item.name.to_lowercase();
        if !tag.is_empty() && !tags.contains(&tag) {
            tags.push(tag);
        }
    }
    (title, tags, created)
}

/// The text of the first ATX heading, without its markers.
fn first_heading(text: &str) -> Option<String> {
    let body = match livemark::parse::front_matter_text(text) {
        Some(front) => text.split_once(front).map_or(text, |(_, rest)| rest),
        None => text,
    };
    body.lines().find_map(|line| {
        let rest = line.trim_start().strip_prefix('#')?;
        let rest = rest.trim_start_matches('#');
        let title = rest
            .strip_prefix([' ', '\t'])?
            .trim()
            .trim_end_matches('#')
            .trim();
        (!title.is_empty()).then(|| title.to_owned())
    })
}

/// The date `s` starts with, `YYYY-MM-DD`, when it is a real one.
fn date_prefix(s: &str) -> Option<String> {
    let date: jiff::civil::Date = s.get(..10)?.parse().ok()?;
    Some(date.to_string())
}

#[cfg(test)]
mod tests {
    use super::{Vault, front_matter, resolve};
    use std::path::Path;

    #[test]
    fn notes_newer_than_the_last_commit_are_counted() {
        let dir = std::env::temp_dir().join(format!("livemark-git-{}", std::process::id()));
        std::fs::create_dir_all(dir.join(".git/logs")).unwrap();
        let note = |name: &str, secs: u64| {
            let path = dir.join(name);
            std::fs::write(&path, "x\n").unwrap();
            let file = std::fs::File::options().write(true).open(&path).unwrap();
            let at = std::time::UNIX_EPOCH + std::time::Duration::from_secs(secs);
            file.set_modified(at).unwrap();
        };
        note("old.md", 1_000);
        note("new.md", 3_000);
        note("newer.md", 4_000);
        // Outside a repository (no log yet): nothing to say.
        let mut vault = Vault::open(&dir).unwrap();
        assert_eq!(vault.uncommitted(), None);
        std::fs::write(
            dir.join(".git/logs/HEAD"),
            "0 a Me <me@x> 500 +0400\tclone\na b Me <me@x> 2000 +0400\tcommit: notes\n",
        )
        .unwrap();
        vault.refresh();
        assert_eq!(vault.uncommitted(), Some(2));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn links_resolve_to_files_next_to_the_note() {
        let from = Path::new("/v/notes/a.md");
        assert_eq!(resolve(from, "b.md"), Some("/v/notes/b.md".into()));
        assert_eq!(resolve(from, "../c%20d.md#part"), Some("/v/c d.md".into()));
        assert_eq!(
            resolve(from, "./sub/e.markdown"),
            Some("/v/notes/sub/e.markdown".into())
        );
        for dest in ["https://x.org/a.md", "mailto:a@b.c", "#top", "pic.png", ""] {
            assert_eq!(resolve(from, dest), None, "{dest:?}");
        }
    }

    #[test]
    fn front_matter_tags_titles_and_dates_are_read_leniently() {
        let read = |yaml: &str| front_matter(&format!("---\n{yaml}---\nText.\n"));
        let (title, tags, created) =
            read("title: \"Lisbon\"\ntags: [Work, travel, '#work']\ncreated: 2026-10-02\n");
        assert_eq!(title.as_deref(), Some("Lisbon"));
        assert_eq!(tags, ["work", "travel"]);
        assert_eq!(created.as_deref(), Some("2026-10-02"));
        assert_eq!(read("tags:\n  - a\n  - b\nx: y\n").1, ["a", "b"]);
        assert_eq!(read("tags: a, b\n").1, ["a", "b"]);
        // Broken lines are skipped, and a date must be one.
        assert_eq!(read("tags\ncreated: soon\n:::\n"), (None, vec![], None));
        assert_eq!(read("created: 2026-13-45\n").2, None);
    }

    #[cfg(unix)]
    #[test]
    fn linked_folders_are_read_and_a_link_loop_is_skipped() {
        let dir = std::env::temp_dir().join(format!("livemark-linked-{}", std::process::id()));
        let (vault, elsewhere) = (dir.join("vault"), dir.join("elsewhere"));
        std::fs::create_dir_all(&vault).unwrap();
        std::fs::create_dir_all(&elsewhere).unwrap();
        std::fs::write(vault.join("near.md"), "# Near\n").unwrap();
        std::fs::write(elsewhere.join("far.md"), "# Far\n").unwrap();
        std::os::unix::fs::symlink(&elsewhere, vault.join("linked")).unwrap();
        std::os::unix::fs::symlink(&vault, vault.join("loop")).unwrap();
        let read = Vault::open(&vault).unwrap();
        let mut titles: Vec<&str> = read.notes.iter().map(|n| n.title.as_str()).collect();
        titles.sort_unstable();
        assert_eq!(titles, ["Far", "Near"]);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_vault_reads_its_notes_and_follows_changes_on_disk() {
        let dir = std::env::temp_dir().join(format!("livemark-vault-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        std::fs::create_dir_all(dir.join(".git")).unwrap();
        let write = |name: &str, text: &str| std::fs::write(dir.join(name), text).unwrap();
        write(
            "2026-10-01-lisbon.md",
            "---\ntitle: Lisbon trip\ntags: [travel]\n---\nSee #work too.\n",
        );
        write("sub/plain.md", "# A heading\n\ntext #travel\n");
        write(".git/HEAD.md", "not a note");
        write("image.png", "not a note");
        let mut vault = Vault::open(&dir).unwrap();
        assert_eq!(vault.notes.len(), 2);
        let lisbon = vault
            .notes
            .iter()
            .find(|n| n.title == "Lisbon trip")
            .unwrap();
        assert_eq!(lisbon.tags, ["travel", "work"]);
        assert_eq!(lisbon.created.as_deref(), Some("2026-10-01"));
        let plain = vault.notes.iter().find(|n| n.title == "A heading").unwrap();
        assert_eq!(plain.created, None);
        assert_eq!(
            vault.tags(),
            [("travel".to_owned(), 2), ("work".to_owned(), 1)]
        );
        // Nothing changed: nothing read again.
        let generation = vault.generation;
        assert!(!vault.refresh());
        assert_eq!(vault.generation, generation);
        // A new note and a deleted one.
        write("new.md", "new #idea\n");
        std::fs::remove_file(dir.join("sub/plain.md")).unwrap();
        assert!(vault.refresh());
        assert!(vault.notes.iter().any(|n| n.title == "new"));
        assert_eq!(vault.notes.len(), 2);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
