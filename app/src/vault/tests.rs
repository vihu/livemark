//! Reading a vault: titles, tags, dates, links and the commit status.
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
    // A long log: only its end is read, the last line still found.
    let mut log = "0 a Me <me@x> 500 +0400\tcommit: an older one\n".repeat(200);
    log.push_str("a b Me <me@x> 3500 +0400\tcommit: more\n");
    std::fs::write(dir.join(".git/logs/HEAD"), log).unwrap();
    vault.refresh();
    assert_eq!(vault.uncommitted(), Some(1));
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_note_written_by_the_app_is_read_again_alone_and_bad_files_are_counted() {
    let dir = std::env::temp_dir().join(format!("livemark-reread-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("a.md"), "# A\n").unwrap();
    std::fs::write(dir.join("latin.md"), b"caf\xe9\n").unwrap();
    let mut vault = Vault::open(&dir).unwrap();
    let a = vault.root.join("a.md");
    assert_eq!((vault.notes.len(), vault.skipped.len()), (1, 1));
    // Nothing changed: no new generation, the file not read again.
    let generation = vault.generation;
    assert!(!vault.refresh());
    assert_eq!(vault.generation, generation);
    // Written here: read again without a walk; a path it lacks is left
    // to a refresh.
    std::fs::write(&a, "# A, longer\n#tag\n").unwrap();
    assert!(vault.reread(&a));
    assert_eq!(vault.notes[0].title, "A, longer");
    assert_eq!(vault.tags(), [("tag".into(), 1)]);
    assert!(vault.generation > generation);
    assert!(!vault.reread(&vault.root.join("elsewhere.md")));
    // A skipped file mended: a note again.
    std::fs::write(dir.join("latin.md"), "café\n").unwrap();
    assert!(vault.refresh());
    assert_eq!((vault.notes.len(), vault.skipped.len()), (2, 0));
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
    // Names with what a link reads otherwise, written by `link_path`
    // (found in REVIEW-001: only spaces were encoded).
    for name in ["C# notes.md", "why?.md", "50% (draft).md", "café.md"] {
        let dest = crate::file::link_path(name);
        assert_eq!(
            resolve(from, &dest),
            Some(Path::new("/v/notes").join(name)),
            "{dest}"
        );
    }
    assert_eq!(crate::file::link_path("café (1).md"), "café%20%281%29.md");
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
