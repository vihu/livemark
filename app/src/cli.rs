//! The command line for scripts and agents (PLAN-004 answer 6): no
//! window, never a prompt, never a file overwritten; what was made on
//! stdout, errors on stderr with a non-zero exit.
//!
//! ```text
//! livemark note "<title>" [--tags a,b] [--by <agent>] [--vault <dir>] < body.md
//! livemark tags [--vault <dir>]
//! livemark --help
//! ```
//!
//! The vault is `--vault`, else the one the app last opened (settings).
use std::io::{IsTerminal, Read};
use std::path::PathBuf;

use super::note::{self, Header};
use super::settings::Settings;
use super::vault::Vault;

const USAGE: &str = "\
livemark [file.md] [--dark|--light]     open the editor
livemark note \"<title>\" [--tags a,b] [--by <agent>] [--vault <dir>] < body.md
                                        write a new note into the vault; prints its path
livemark tags [--vault <dir>]           the vault's tags, most used first: `tag count` a line
livemark --help                         this

The vault is --vault, else the one the app last opened. A note is never
overwritten: a second one of the same title and day gets -2. Notes are
named YYYY-MM-DD-title-in-lowercase.md, with front matter for the title,
tags, date and the agent that wrote it (--by).";

/// Runs a command when `args` (without the program's name) is one: its
/// exit code. `None` for anything else: the editor opens.
pub fn run(args: &[String]) -> Option<i32> {
    let outcome = match args.first().map(String::as_str)? {
        "note" => {
            let mut body = String::new();
            // Never waiting on a terminal: no body then.
            if !std::io::stdin().is_terminal() {
                let _ = std::io::stdin().read_to_string(&mut body);
            }
            note_command(&args[1..], &body, saved_vault())
        }
        "tags" => tags_command(&args[1..], saved_vault()),
        "--help" | "-h" | "help" => Ok(USAGE.to_owned()),
        _ => return None,
    };
    Some(match outcome {
        Ok(out) => {
            println!("{out}");
            0
        }
        Err(error) => {
            eprintln!("livemark: {error}");
            1
        }
    })
}

/// The vault the app last opened, from its settings.
fn saved_vault() -> Option<PathBuf> {
    Settings::path()
        .map(|file| Settings::load(&file))
        .and_then(|s| s.vault)
}

/// `--name value` pairs and the other arguments, in order.
type Parsed<'a> = (Vec<(&'a str, &'a str)>, Vec<&'a str>);

/// `--name value` pairs and the rest, in order.
fn options(args: &[String]) -> Result<Parsed<'_>, String> {
    let mut named = Vec::new();
    let mut rest = Vec::new();
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        match arg.strip_prefix("--") {
            Some(name @ ("tags" | "by" | "vault")) => {
                let value = args.next().ok_or(format!("--{name} needs a value"))?;
                named.push((name, value.as_str()));
            }
            Some(other) => return Err(format!("unknown option --{other}\n\n{USAGE}")),
            None => rest.push(arg.as_str()),
        }
    }
    Ok((named, rest))
}

fn vault_from(named: &[(&str, &str)], saved: Option<PathBuf>) -> Result<Vault, String> {
    let root = named
        .iter()
        .find(|(name, _)| *name == "vault")
        .map(|(_, value)| PathBuf::from(value))
        .or(saved)
        .ok_or("no vault: pass --vault <dir>, or open one in the app")?;
    Vault::open(&root).map_err(|e| e.to_string())
}

/// `note`: the new note's path.
fn note_command(args: &[String], body: &str, saved: Option<PathBuf>) -> Result<String, String> {
    let (named, rest) = options(args)?;
    let [title] = rest[..] else {
        return Err(format!("note takes one title, in quotes\n\n{USAGE}"));
    };
    let title = title.trim();
    if title.is_empty() || title.contains(['\n', '\r']) {
        return Err("the title is one non-empty line".into());
    }
    let mut tags: Vec<String> = Vec::new();
    for (_, value) in named.iter().filter(|(name, _)| *name == "tags") {
        for tag in value.split([',', ' ']) {
            let tag = tag.trim().trim_start_matches('#').to_lowercase();
            if !tag.is_empty() && !tags.contains(&tag) {
                tags.push(tag);
            }
        }
    }
    let by = named
        .iter()
        .find(|(name, _)| *name == "by")
        .map(|(_, v)| v.trim());
    if by.is_some_and(|by| by.is_empty() || by.contains(['\n', '\r', ':'])) {
        return Err("--by is a plain name, like claude-code".into());
    }
    let vault = vault_from(&named, saved)?;
    let header = Header {
        title,
        tags: &tags,
        created: &note::today(),
        by,
    };
    let path = note::create(&vault.root, &header, body.trim_start_matches(['\n', '\r']))
        .map_err(|e| format!("{}: {e}", vault.root.display()))?;
    Ok(path.display().to_string())
}

/// `tags`: one `tag count` a line, most used first.
fn tags_command(args: &[String], saved: Option<PathBuf>) -> Result<String, String> {
    let (named, rest) = options(args)?;
    if !rest.is_empty() {
        return Err(format!("tags takes no arguments\n\n{USAGE}"));
    }
    let vault = vault_from(&named, saved)?;
    Ok(vault
        .tags()
        .iter()
        .map(|(tag, count)| format!("{tag} {count}"))
        .collect::<Vec<_>>()
        .join("\n"))
}

#[cfg(test)]
mod tests {
    use super::{note_command, run, tags_command};

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn note_writes_a_dated_file_and_tags_lists_the_vaults_tags() {
        let dir = std::env::temp_dir().join(format!("livemark-cli-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let vault = dir.to_string_lossy().into_owned();
        let call = |list: &[&str], body: &str| {
            let mut all = args(list);
            all.extend(args(&["--vault", &vault]));
            note_command(&all, body, None)
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
        let listed = tags_command(&args(&["--vault", &vault]), None).unwrap();
        assert_eq!(listed, "ops 1\nwork 1");
        // Mistakes are said, nothing written.
        assert!(call(&[], "").unwrap_err().contains("one title"));
        assert!(call(&["T", "--by", "a:b"], "").is_err());
        assert!(
            call(&["T", "--colour", "red"], "")
                .unwrap_err()
                .contains("--colour")
        );
        assert!(
            note_command(&args(&["T"]), "", None)
                .unwrap_err()
                .contains("no vault")
        );
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 2);
        // Not a command: the editor opens.
        assert_eq!(run(&args(&["notes.md"])), None);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
