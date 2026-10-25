//! A vault (PLAN-004): a folder of markdown notes, read into memory, no
//! database and no files of the app's own in it. Each note's title, tags
//! and creation date come from its front matter (`title`, `tags`,
//! `created`), else its first heading and its file name's date; inline
//! `#tags` count too. Read again by modification time when asked
//! (`refresh`), so changes from git, other editors or agents show up.
use std::collections::{BTreeMap, HashMap};
use std::io::{Read as _, Seek as _};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use livemark::parse::properties::{properties, unquoted};

/// A note as the vault knows it.
#[derive(Debug, Clone)]
pub struct Note {
    pub path: PathBuf,
    pub title: String,
    /// The title's words, for the search field and `[[` completion
    /// (`matching::words_of`), worked out once.
    pub title_words: Vec<(std::ops::Range<usize>, Vec<char>)>,
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
    /// Markdown files that did not read (not UTF-8, not readable), with
    /// their time and size: said at the sidebar's foot, read again only
    /// when they change.
    pub skipped: Vec<(PathBuf, SystemTime, u64)>,
    /// [`Vault::tags`], counted again when a refresh changes something.
    tags: Vec<(String, usize)>,
    /// [`Vault::look_alikes`], worked out when first asked for after the
    /// tags changed.
    alike: std::cell::OnceCell<Vec<(String, String)>>,
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
            skipped: Vec::new(),
            tags: Vec::new(),
            alike: std::cell::OnceCell::new(),
        };
        vault.refresh();
        Ok(vault)
    }

    /// Reads again the notes whose files changed, came or went; returns
    /// whether anything did. When none did, the notes stay as they are.
    pub fn refresh(&mut self) -> bool {
        let files = walk(&self.root);
        let last_commit = last_commit(&self.root);
        let committed = last_commit != self.last_commit;
        self.last_commit = last_commit;
        if self.holds(&files) {
            if committed {
                self.generation += 1;
            }
            return committed;
        }
        let mut old: HashMap<PathBuf, Note> = std::mem::take(&mut self.notes)
            .into_iter()
            .map(|note| (note.path.clone(), note))
            .collect();
        let old_skipped: HashMap<PathBuf, (SystemTime, u64)> = std::mem::take(&mut self.skipped)
            .into_iter()
            .map(|(path, modified, size)| (path, (modified, size)))
            .collect();
        for (path, modified, size) in files {
            match old.remove(&path) {
                Some(note) if (note.modified, note.size) == (modified, size) => {
                    self.notes.push(note);
                }
                _ if old_skipped.get(&path) == Some(&(modified, size)) => {
                    self.skipped.push((path, modified, size));
                }
                _ => match std::fs::read_to_string(&path) {
                    Ok(text) => self.notes.push(read(path, modified, text)),
                    Err(_) => self.skipped.push((path, modified, size)),
                },
            }
        }
        self.changed();
        true
    }

    /// Whether `files` (from `walk`) are the notes and skipped files the
    /// vault has, each at the time and size it was read.
    fn holds(&self, files: &[(PathBuf, SystemTime, u64)]) -> bool {
        if files.len() != self.notes.len() + self.skipped.len() {
            return false;
        }
        let known: HashMap<&Path, (SystemTime, u64)> = self
            .notes
            .iter()
            .map(|note| (note.path.as_path(), (note.modified, note.size)))
            .chain(
                self.skipped
                    .iter()
                    .map(|(path, m, s)| (path.as_path(), (*m, *s))),
            )
            .collect();
        files
            .iter()
            .all(|(path, m, s)| known.get(path.as_path()) == Some(&(*m, *s)))
    }

    /// The note at `path` read again after the app wrote it, with no walk
    /// over the vault; `false` when the vault has no note there (for a
    /// refresh to find it).
    pub fn reread(&mut self, path: &Path) -> bool {
        let Some(at) = self.notes.iter().position(|note| note.path == path) else {
            return false;
        };
        let Ok((modified, size)) =
            std::fs::metadata(path).and_then(|m| Ok((m.modified()?, m.len())))
        else {
            return false;
        };
        if (self.notes[at].modified, self.notes[at].size) == (modified, size) {
            return true;
        }
        let Ok(text) = std::fs::read_to_string(path) else {
            return false;
        };
        self.notes[at] = read(path.to_path_buf(), modified, text);
        self.changed();
        true
    }

    /// After notes changed: in order again, counted again.
    fn changed(&mut self) {
        // Then newest created, then by title: a fresh clone gives every
        // file the same time.
        self.notes.sort_by(|a, b| {
            (b.modified, &b.created)
                .cmp(&(a.modified, &a.created))
                .then_with(|| a.title.cmp(&b.title))
        });
        self.generation += 1;
        let tags = count_tags(&self.notes);
        if tags != self.tags {
            self.tags = tags;
            self.alike = std::cell::OnceCell::new();
        }
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
    /// name, as of the last refresh.
    pub fn tags(&self) -> &[(String, usize)] {
        &self.tags
    }

    /// Every pair of tags that look alike (`manager::look_alikes`), worked
    /// out once for each change to the tags.
    pub fn look_alikes(&self) -> &[(String, String)] {
        self.alike
            .get_or_init(|| super::manager::look_alikes(&self.tags))
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
    // Its last line only: the log grows by a line a commit, for good.
    let mut log = std::fs::File::open(git.join("logs").join("HEAD")).ok()?;
    let len = log.metadata().ok()?.len();
    log.seek(std::io::SeekFrom::Start(len.saturating_sub(4096)))
        .ok()?;
    let mut tail = Vec::new();
    log.read_to_end(&mut tail).ok()?;
    let tail = String::from_utf8_lossy(&tail);
    let line = tail.lines().last()?;
    let head = line.split('\t').next()?;
    let seconds: u64 = head.rsplit(' ').nth(1)?.parse().ok()?;
    Some(SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(seconds))
}

/// Every tag of `notes` with how many have it, most used first, then by
/// name.
fn count_tags(notes: &[Note]) -> Vec<(String, usize)> {
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for tag in notes.iter().flat_map(|note| &note.tags) {
        *counts.entry(tag).or_default() += 1;
    }
    let mut tags: Vec<(String, usize)> = counts
        .into_iter()
        .map(|(tag, count)| (tag.to_owned(), count))
        .collect();
    tags.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    tags
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
        .filter(|entry| entry.file_type().is_file() && super::file::is_markdown(entry.path()))
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
        .or_else(|| livemark::parse::heading(&text).map(|r| text[r].to_owned()))
        .unwrap_or_else(|| stem.clone());
    let created = created.or_else(|| date_prefix(&stem));
    let links = livemark::parse::links(&text)
        .iter()
        .filter_map(|dest| resolve(&path, dest))
        .collect();
    Note {
        path,
        title_words: super::matching::words_of(&title),
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
    if !super::file::is_markdown(target) {
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

/// The date `s` starts with, `YYYY-MM-DD`, when it is a real one.
fn date_prefix(s: &str) -> Option<String> {
    let date: jiff::civil::Date = s.get(..10)?.parse().ok()?;
    Some(date.to_string())
}

#[cfg(test)]
mod tests;
