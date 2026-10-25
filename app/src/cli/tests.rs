//! The command line's commands, run without a process: parsing, notes,
//! tags, lists, searches and pictures.
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
fn search_prints_up_to_its_limit_and_tags_must_read_as_tags() {
    let dir = std::env::temp_dir().join(format!("livemark-cli-many-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    // More than the app's sidebar keeps (`search::NOTES`).
    for n in 0..crate::search::NOTES + 5 {
        std::fs::write(dir.join(format!("n{n}.md")), "a word\n").unwrap();
    }
    let vault = dir.to_string_lossy().into_owned();
    let found = |limit: &str| match parse(&["search", "word", "--limit", limit, "--vault", &vault])
        .unwrap()
        .command
    {
        Some(Command::Search { words, filter }) => search_notes(&words, &filter).unwrap(),
        other => panic!("{other:?}"),
    };
    assert_eq!(found("500").lines().count(), crate::search::NOTES + 5);
    assert_eq!(found("3").lines().count(), 3);
    // Brackets and the like would break the YAML list.
    let error = write_note("T", &["a]b".into()], None, Some(dir.clone()), "").unwrap_err();
    assert!(error.contains("a]b is not a tag"), "{error}");
    assert!(write_note("T", &["2026".into()], None, Some(dir.clone()), "").is_err());
    assert!(write_note("T", &["work/q4, c-2".into()], None, Some(dir.clone()), "").is_ok());
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
