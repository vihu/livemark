//! The command line (PLAN-004 answer 6), parsed by `clap`: a file to
//! open with an optional theme, or a command for scripts and agents, which
//! opens no window, never prompts and never overwrites a file; what was
//! made on stdout, errors on stderr with a non-zero exit.
//!
//! ```text
//! livemark [file.md] [--dark|--light]
//! livemark note "<title>" [--tags a,b] [--by <agent>] [--vault <dir>] < body.md
//! livemark tags [--vault <dir>]
//! ```
//!
//! The vault is `--vault`, else the one the app last opened (settings).
use std::io::{IsTerminal, Read};
use std::path::PathBuf;

use clap::{Parser, Subcommand};

use super::note::{self, Header};
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
    };
    match outcome {
        Ok(out) => {
            println!("{out}");
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
    let mut clean: Vec<String> = Vec::new();
    for tag in tags.iter().flat_map(|value| value.split([',', ' '])) {
        let tag = tag.trim().trim_start_matches('#').to_lowercase();
        if !tag.is_empty() && !clean.contains(&tag) {
            clean.push(tag);
        }
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

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::{Args, Command, list_tags, write_note};

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
}
