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
use std::io::{IsTerminal, Read};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use clap::{Parser, Subcommand, ValueEnum};

use super::file;
use super::note::{self, Header};
use super::search::{search, tagged};
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
            // Never waiting on a terminal: no body then.
            if !std::io::stdin().is_terminal() {
                let _ = std::io::stdin().read_to_string(&mut body);
            }
            write_note(
                &title,
                &tags,
                by.as_deref(),
                vault.or_else(saved_vault),
                &body,
            )
        }
        Command::Tags { vault } => list_tags(vault.or_else(saved_vault)),
        Command::List { filter } => list_notes(&filter),
        Command::Search { words, filter } => search_notes(&words, &filter),
        Command::Picture { note, files } => add_pictures(&note, &files),
    };
    match outcome {
        Ok(out) => {
            // Nothing found prints nothing.
            if !out.is_empty() {
                println!("{out}");
            }
            0
        }
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
    let (hits, count) = search(&vault, &words.join(" "));
    left_out(filter.limit, count);
    let mut out = Vec::new();
    for hit in hits.iter().take(filter.limit) {
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
mod tests {
    use clap::Parser;

    use super::{Args, Command, add_pictures, list_notes, list_tags, search_notes, write_note};

    fn parse(list: &[&str]) -> Result<Args, String> {
        Args::try_parse_from(std::iter::once("livemark").chain(list.iter().copied()))
            .map_err(|e| e.to_string())
    }

    #[test]
    fn note_writes_a_dated_file_and_tags_lists_the_vaults_tags() {
        let dir = std::env::temp_dir().join(format!("livemark-cli-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let vault = dir.to_string_lossy().into_owned();
        let call = |list: &[&str], body: &str| {
            let mut all = vec!["note"];
            all.extend(list);
            all.extend(["--vault", &vault]);
            match parse(&all)?.command {
                Some(Command::Note {
                    title,
                    tags,
                    by,
                    vault,
                }) => write_note(&title, &tags, by.as_deref(), vault, body),
                other => panic!("{other:?}"),
            }
        };
        let path = call(
            &[
                "Deploy key rotated",
                "--tags",
                "Work, #ops",
                "--by",
                "claude-code",
            ],
            "\nRotated on staging.\n",
        )
        .unwrap();
        assert!(path.ends_with("-deploy-key-rotated.md"), "{path}");
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.starts_with("---\ntitle: Deploy key rotated\ntags: [work, ops]\ncreated: "));
        assert!(
            text.ends_with("by: claude-code\n---\n\nRotated on staging.\n"),
            "{text}"
        );
        // The same again: never over the first.
        let again = call(&["Deploy key rotated"], "").unwrap();
        assert!(again.ends_with("-deploy-key-rotated-2.md"));
        assert_eq!(list_tags(Some(dir.clone())).unwrap(), "ops 1\nwork 1");
        // Mistakes are said, nothing written.
        assert!(call(&[], "").unwrap_err().contains("<TITLE>"));
        assert!(call(&["T", "--by", "a:b"], "").is_err());
        assert!(
            call(&["T", "--colour", "red"], "")
                .unwrap_err()
                .contains("--colour")
        );
        assert!(
            write_note("T", &[], None, None, "")
                .unwrap_err()
                .contains("no vault")
        );
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 2);
        // Not a command: the editor opens, with the file and theme asked for.
        let args = parse(&["notes.md", "--dark"]).unwrap();
        assert!(args.command.is_none() && args.dark && !args.light);
        assert_eq!(args.file.as_deref(), Some(std::path::Path::new("notes.md")));
        assert!(parse(&["--dark", "--light"]).is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn list_and_search_keep_the_tags_asked_for_newest_first() {
        let dir = std::env::temp_dir().join(format!("livemark-cli-list-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        // Changed in this order; their own dates run the other way.
        let notes = [
            ("undated.md", "# Undated\n\nThe deploy key moved.\n"),
            (
                "2026-03-01-lisbon.md",
                "---\ntitle: Lisbon\ntags: [travel]\n---\n\nNo deploy here.\n",
            ),
            (
                "2026-01-01-key.md",
                "---\ntitle: Key rotated\ntags: [work, ops]\ncreated: 2026-01-01\n---\n\n\
                 The deploy key rotated.\n#later\n",
            ),
        ];
        let start = std::time::SystemTime::now() - std::time::Duration::from_secs(3600);
        let changed = |n: usize| start + std::time::Duration::from_secs(60 * n as u64);
        for (n, (name, text)) in notes.iter().enumerate() {
            let file = dir.join(name);
            std::fs::write(&file, text).unwrap();
            let file = std::fs::File::options().write(true).open(&file).unwrap();
            file.set_modified(changed(n)).unwrap();
        }
        let vault = dir.to_string_lossy().into_owned();
        let call = |list: &[&str]| {
            let mut all = list.to_vec();
            all.extend(["--vault", &vault]);
            match parse(&all)?.command {
                Some(Command::List { filter }) => list_notes(&filter),
                Some(Command::Search { words, filter }) => search_notes(&words, &filter),
                other => panic!("{other:?}"),
            }
        };
        let column = |list: &[&str], at: usize| -> Vec<String> {
            let out = call(list).unwrap();
            out.lines()
                .map(|line| line.split('\t').nth(at).unwrap().to_owned())
                .collect()
        };
        let root = dir.canonicalize().unwrap();
        let [undated, lisbon, key] = ["undated.md", "2026-03-01-lisbon.md", "2026-01-01-key.md"]
            .map(|name| root.join(name).display().to_string());

        // Last changed first, with the day it changed, the title and tags.
        assert_eq!(column(&["list"], 1), [key.as_str(), &lisbon, &undated]);
        assert_eq!(
            call(&["list", "--limit", "1"]).unwrap(),
            format!(
                "{}\t{key}\tKey rotated\t#work #ops #later",
                super::day(changed(2))
            )
        );
        // By the note's own date, the undated last.
        assert_eq!(
            column(&["list", "--sort", "created"], 0),
            ["2026-03-01", "2026-01-01", "-"]
        );
        // Tags by their start, any case, `#` and commas allowed; all must match.
        assert_eq!(column(&["list", "--tag", "OPS"], 1), [key.as_str()]);
        assert_eq!(column(&["list", "--tag", "#wo,lat"], 1), [key.as_str()]);
        assert_eq!(
            call(&["list", "--tag", "ops", "--tag", "travel"]).unwrap(),
            ""
        );

        // Every word in the note; each line with one, as `grep -n`.
        assert_eq!(
            call(&["search", "deploy", "key"]).unwrap(),
            format!(
                "{key}:2:title: Key rotated\n{key}:7:The deploy key rotated.\n\
                 {undated}:3:The deploy key moved."
            )
        );
        assert_eq!(
            call(&["search", "deploy", "--limit", "1"]).unwrap(),
            format!("{key}:7:The deploy key rotated.")
        );
        assert_eq!(
            call(&["search", "DEPLOY", "--sort", "created", "--limit", "1"]).unwrap(),
            format!("{lisbon}:6:No deploy here.")
        );
        assert_eq!(
            call(&["search", "deploy", "--tag", "travel"]).unwrap(),
            format!("{lisbon}:6:No deploy here.")
        );
        // A tag alone: the note's path.
        assert_eq!(call(&["search", "#ops"]).unwrap(), key);
        assert_eq!(call(&["search", "nowhere"]).unwrap(), "");
        assert!(parse(&["search"]).is_err());
        assert!(parse(&["list", "--sort", "title"]).is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn pictures_are_copied_next_to_the_note_and_never_over_a_file() {
        let dir = std::env::temp_dir().join(format!("livemark-cli-pic-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let note = dir.join("2026-10-06-flame-graph.md");
        std::fs::write(&note, "# Flame graph\n").unwrap();
        let [png, jpg, text] = ["shot.png", "Photo.JPG", "notes.txt"].map(|name| dir.join(name));
        std::fs::write(&png, b"png").unwrap();
        std::fs::write(&jpg, b"jpg").unwrap();
        std::fs::write(&text, b"text").unwrap();
        let call = |list: &[&std::path::Path]| {
            let mut all = vec!["picture".to_owned()];
            all.extend(list.iter().map(|p| p.display().to_string()));
            match Args::try_parse_from(std::iter::once("livemark".to_owned()).chain(all)) {
                Ok(Args {
                    command: Some(Command::Picture { note, files }),
                    ..
                }) => add_pictures(&note, &files),
                other => panic!("{other:?}"),
            }
        };
        assert_eq!(
            call(&[&note, &png, &jpg]).unwrap(),
            "![](assets/2026-10-06-flame-graph-1.png)\n![](assets/2026-10-06-flame-graph-1.jpg)"
        );
        // Again: new names, the first copies untouched.
        assert_eq!(
            call(&[&note, &png]).unwrap(),
            "![](assets/2026-10-06-flame-graph-2.png)"
        );
        let assets = dir.join("assets");
        assert_eq!(
            std::fs::read(assets.join("2026-10-06-flame-graph-1.jpg")).unwrap(),
            b"jpg"
        );
        // A wrong file keeps none, even the right ones before it.
        assert!(
            call(&[&note, &png, &text])
                .unwrap_err()
                .contains("notes.txt")
        );
        assert!(call(&[&note, &dir.join("gone.png")]).is_err());
        assert!(
            call(&[&dir.join("gone.md"), &png])
                .unwrap_err()
                .contains("no such note")
        );
        assert_eq!(std::fs::read_dir(&assets).unwrap().count(), 3);
        assert!(parse(&["picture", "note.md"]).is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
