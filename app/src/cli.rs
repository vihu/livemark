//! The command line (PLAN-004 answer 6), parsed by `clap`: a file to
//! open with an optional theme, or a command for scripts and agents, which
//! opens no window, never prompts and never overwrites a file; what was
//! made or found on stdout, errors on stderr with a non-zero exit.
//!
//! ```text
//! livemark [file.md] [--dark|--light]
//! livemark note "<title>" [--tags a,b] [--by <agent>] [--vault <dir>] < body.md
//! livemark tags [--vault <dir>]
//! livemark list [--tag t]... [--sort modified|created] [--limit 20] [--vault <dir>]
//! livemark search <words>... [--tag t]... [--sort ...] [--limit 20] [--vault <dir>]
//! livemark picture <note.md> <file>...
//! ```
//!
//! The vault is `--vault`, else the one the app last opened (settings).
use std::io::{IsTerminal, Read, Write};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use clap::{Parser, Subcommand, ValueEnum};

use super::file;
use super::note::{self, Header};
use super::search::{LINES, search, tagged};
use super::settings::Settings;
use super::vault::Vault;

/// A live-preview markdown editor.
#[derive(Debug, Parser)]
#[command(
    name = "livemark",
    version,
    args_conflicts_with_subcommands = true,
    after_help = "The vault is --vault, else the one the app last opened. A note is never \
overwritten: a second one of the same title and day gets -2. Notes are named \
YYYY-MM-DD-title-in-lowercase.md, with front matter for the title, tags, date and the \
agent that wrote it (--by)."
)]
pub struct Args {
    /// The note to open.
    pub file: Option<PathBuf>,
    /// Dark theme for this run.
    #[arg(long, conflicts_with = "light")]
    pub dark: bool,
    /// Light theme for this run.
    #[arg(long)]
    pub light: bool,
    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Write a new note into the vault, its body from standard input; prints its path.
    Note {
        /// The title, one line.
        title: String,
        /// Its tags, separated by commas or spaces; may be given again.
        #[arg(long)]
        tags: Vec<String>,
        /// Who wrote it, a plain name like claude-code.
        #[arg(long)]
        by: Option<String>,
        /// The vault's folder.
        #[arg(long)]
        vault: Option<PathBuf>,
    },
    /// The vault's tags, most used first: `tag count` a line.
    Tags {
        /// The vault's folder.
        #[arg(long)]
        vault: Option<PathBuf>,
    },
    /// The vault's notes, newest first: `date<TAB>path<TAB>title<TAB>#tags` a line.
    List {
        #[command(flatten)]
        filter: Filter,
    },
    /// The notes with every word (`#tag` narrows), newest first: `path:line:text` a line.
    Search {
        /// The words to find, in any case, anywhere in a note.
        #[arg(required = true)]
        words: Vec<String>,
        #[command(flatten)]
        filter: Filter,
    },
    /// Copies pictures into the `assets` folder next to a note; prints a markdown image line each.
    Picture {
        /// The note the pictures are for.
        note: PathBuf,
        /// PNG, JPEG, GIF or WebP files, at most 32 MB each.
        #[arg(required = true)]
        files: Vec<PathBuf>,
    },
}

/// Which notes `list` and `search` print, and in what order.
#[derive(Debug, clap::Args)]
pub struct Filter {
    /// Only notes with this tag, or one starting with it; may be given again.
    #[arg(long)]
    tag: Vec<String>,
    /// Newest first by the file's last change, or by the note's own date.
    #[arg(long, value_enum, default_value_t = Sort::Modified)]
    sort: Sort,
    /// At most this many notes.
    #[arg(long, default_value_t = 20)]
    limit: usize,
    /// The vault's folder.
    #[arg(long)]
    vault: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Sort {
    /// The file's last change.
    Modified,
    /// The note's `created` date; notes without one last.
    Created,
}

/// Runs `command`: its exit code.
pub fn run(command: Command) -> i32 {
    let outcome = match command {
        Command::Note {
            title,
            tags,
            by,
            vault,
        } => {
            let mut body = String::new();
            // Never waiting on a terminal: no body then. A body that does
            // not read (not UTF-8) writes no note.
            let read = if std::io::stdin().is_terminal() {
                Ok(0)
            } else {
                std::io::stdin().read_to_string(&mut body)
            };
            match read {
                Ok(_) => write_note(
                    &title,
                    &tags,
                    by.as_deref(),
                    vault.or_else(saved_vault),
                    &body,
                ),
                Err(e) => Err(format!("the body on standard input: {e}")),
            }
        }
        Command::Tags { vault } => list_tags(vault.or_else(saved_vault)),
        Command::List { filter } => list_notes(&filter),
        Command::Search { words, filter } => search_notes(&words, &filter),
        Command::Picture { note, files } => add_pictures(&note, &files),
    };
    match outcome {
        // Nothing found prints nothing.
        Ok(out) if out.is_empty() => 0,
        Ok(out) => match writeln!(std::io::stdout().lock(), "{out}") {
            Ok(()) => 0,
            // The reader stopped early (`| head`): nothing more to say.
            Err(e) if e.kind() == std::io::ErrorKind::BrokenPipe => 0,
            Err(e) => {
                eprintln!("livemark: standard output: {e}");
                1
            }
        },
        Err(error) => {
            eprintln!("livemark: {error}");
            1
        }
    }
}

/// The vault the app last opened, from its settings.
fn saved_vault() -> Option<PathBuf> {
    Settings::path()
        .and_then(|file| Settings::load(&file).ok())
        .and_then(|s| s.vault)
}

fn open_vault(root: Option<PathBuf>) -> Result<Vault, String> {
    let root = root.ok_or("no vault: pass --vault <dir>, or open one in the app")?;
    Vault::open(&root).map_err(|e| e.to_string())
}

/// `note`: the new note's path.
fn write_note(
    title: &str,
    tags: &[String],
    by: Option<&str>,
    vault: Option<PathBuf>,
    body: &str,
) -> Result<String, String> {
    let title = title.trim();
    if title.is_empty() || title.contains(['\n', '\r']) {
        return Err("the title is one non-empty line".into());
    }
    let clean = clean_tags(tags);
    // As `#tags` are read: written into the YAML list, nothing else fits.
    if let Some(bad) = clean.iter().find(|tag| {
        let hashed = format!("#{tag}");
        livemark::parse::tag_at(&hashed, 0) != Some(0..hashed.len())
    }) {
        return Err(format!(
            "--tags: {bad} is not a tag (letters, digits, _, - and /, not only digits)"
        ));
    }
    let by = by.map(str::trim);
    if by.is_some_and(|by| by.is_empty() || by.contains(['\n', '\r', ':'])) {
        return Err("--by is a plain name, like claude-code".into());
    }
    let vault = open_vault(vault)?;
    let header = Header {
        title,
        tags: &clean,
        created: &note::today(),
        by,
    };
    let path = note::create(&vault.root, &header, body.trim_start_matches(['\n', '\r']))
        .map_err(|e| format!("{}: {e}", vault.root.display()))?;
    Ok(path.display().to_string())
}

/// `tags`: one `tag count` a line, most used first.
fn list_tags(vault: Option<PathBuf>) -> Result<String, String> {
    let vault = open_vault(vault)?;
    Ok(vault
        .tags()
        .iter()
        .map(|(tag, count)| format!("{tag} {count}"))
        .collect::<Vec<_>>()
        .join("\n"))
}

/// Tags as given, separated by commas or spaces, `#` optional: lowercase,
/// each once.
fn clean_tags(tags: &[String]) -> Vec<String> {
    let mut clean: Vec<String> = Vec::new();
    for tag in tags.iter().flat_map(|value| value.split([',', ' '])) {
        let tag = tag.trim().trim_start_matches('#').to_lowercase();
        if !tag.is_empty() && !clean.contains(&tag) {
            clean.push(tag);
        }
    }
    clean
}

/// The vault with only the notes `filter` keeps, in its order.
fn filtered(filter: &Filter) -> Result<Vault, String> {
    let mut vault = open_vault(filter.vault.clone().or_else(saved_vault))?;
    let tags = clean_tags(&filter.tag);
    vault.notes.retain(|note| tagged(note, &tags));
    if filter.sort == Sort::Created {
        // Stable: notes of one day stay newest changed first.
        vault.notes.sort_by(|a, b| b.created.cmp(&a.created));
    }
    Ok(vault)
}

/// `list`: one `date<TAB>path<TAB>title<TAB>#tags` a line, the date the
/// one sorted by.
fn list_notes(filter: &Filter) -> Result<String, String> {
    let vault = filtered(filter)?;
    left_out(filter.limit, vault.notes.len());
    Ok(vault
        .notes
        .iter()
        .take(filter.limit)
        .map(|note| {
            let date = match filter.sort {
                Sort::Modified => day(note.modified),
                Sort::Created => note.created.clone().unwrap_or_else(|| "-".into()),
            };
            let tags: Vec<String> = note.tags.iter().map(|tag| format!("#{tag}")).collect();
            format!(
                "{date}\t{}\t{}\t{}",
                note.path.display(),
                note.title,
                tags.join(" ")
            )
        })
        .collect::<Vec<_>>()
        .join("\n"))
}

/// `search`: one `path:line:text` a line, as `grep -n`, the text around
/// the match; a note matched by `#tag` alone is its path.
fn search_notes(words: &[String], filter: &Filter) -> Result<String, String> {
    let vault = filtered(filter)?;
    let (hits, count) = search(&vault, &words.join(" "), filter.limit, LINES);
    left_out(filter.limit, count);
    let mut out = Vec::new();
    for hit in &hits {
        let path = hit.path.display();
        if hit.lines.is_empty() {
            out.push(path.to_string());
        }
        for (number, text, _) in &hit.lines {
            out.push(format!("{path}:{number}:{text}"));
        }
    }
    Ok(out.join("\n"))
}

/// `picture`: each file kept in `assets` next to `note` (never over a
/// file), one `![](assets/...)` line each. Every file is read before any
/// is kept, so a wrong one keeps none.
fn add_pictures(note: &Path, files: &[PathBuf]) -> Result<String, String> {
    if !note.is_file() {
        return Err(format!("{}: no such note", note.display()));
    }
    let mut read = Vec::new();
    for path in files {
        let extension = path
            .extension()
            .map(|e| e.to_string_lossy().to_ascii_lowercase());
        let (Some(extension), true) = (extension, file::is_picture(path)) else {
            return Err(format!(
                "{}: not a PNG, JPEG, GIF or WebP picture",
                path.display()
            ));
        };
        let size = std::fs::metadata(path)
            .map_err(|e| format!("{}: {e}", path.display()))?
            .len();
        if size > file::PICTURE_MAX {
            return Err(format!("{}: larger than 32 MB", path.display()));
        }
        let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
        read.push((bytes, extension));
    }
    let mut out = Vec::new();
    for (bytes, extension) in &read {
        out.push(format!(
            "![]({})",
            file::save_picture(note, bytes, extension)?
        ));
    }
    Ok(out.join("\n"))
}

/// Says on stderr when `limit` left some of `count` notes out.
fn left_out(limit: usize, count: usize) {
    if count > limit {
        eprintln!("livemark: {limit} of {count} notes shown; --limit for more");
    }
}

/// The local day of `time`, `YYYY-MM-DD`.
fn day(time: SystemTime) -> String {
    jiff::Timestamp::try_from(time).map_or_else(
        |_| "-".into(),
        |t| t.to_zoned(jiff::tz::TimeZone::system()).date().to_string(),
    )
}

#[cfg(test)]
mod tests;
