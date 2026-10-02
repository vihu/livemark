//! A note's menu and what it does, without a window (PLAN-006).
use super::note_actions::{NoteAction, NoteMessage};
use super::tests::write;
use super::{App, Message};

/// A vault of three notes, two linking to the first, with the first open.
fn vault(name: &str) -> (std::path::PathBuf, App) {
    let dir = std::env::temp_dir().join(format!("livemark-{name}-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("sub")).unwrap();
    write(
        &dir.join("2026-09-30-lisbon-hotels.md"),
        "---\ntitle: Lisbon hotels\ntags: [travel]\n---\nShortlist.\n",
        0,
    );
    write(
        &dir.join("plan.md"),
        "# Plan\nSee [the hotels](2026-09-30-lisbon-hotels.md#rooms).\n",
        10,
    );
    write(
        &dir.join("sub/trip.md"),
        "# Trip\n- [ ] [Hotels](../2026-09-30-lisbon-hotels.md)\n",
        20,
    );
    let mut app = App::open(None, None);
    let _ = app.update(Message::Vault(crate::sidebar::VaultMessage::Picked(Some(
        dir.clone(),
    ))));
    let root = app.vault.as_ref().unwrap().root.clone();
    let _ = app.update(Message::Opened(Some(
        root.join("2026-09-30-lisbon-hotels.md"),
    )));
    (root, app)
}

fn note(app: &mut App, message: NoteMessage) {
    let _ = app.update(Message::Note(message));
    let _ = app.view();
}

fn read(path: &std::path::Path) -> String {
    std::fs::read_to_string(path).unwrap()
}

#[test]
fn a_rename_moves_the_file_retitles_it_and_its_links_follow_then_undo() {
    let (root, mut app) = vault("rename");
    let old = root.join("2026-09-30-lisbon-hotels.md");
    let new = root.join("2026-09-30-lisbon-stays.md");
    note(&mut app, NoteMessage::Menu(old.clone()));
    assert_eq!(app.note_menu.as_ref(), Some(&old));
    note(&mut app, NoteMessage::StartRename(Some(old.clone())));
    note(&mut app, NoteMessage::RenameText("Lisbon stays".into()));
    note(&mut app, NoteMessage::Confirm);
    assert!(!old.exists());
    assert_eq!(
        read(&new),
        "---\ntitle: Lisbon stays\ntags: [travel]\n---\nShortlist.\n"
    );
    assert_eq!(
        read(&root.join("plan.md")),
        "# Plan\nSee [the hotels](2026-09-30-lisbon-stays.md#rooms).\n"
    );
    assert_eq!(
        read(&root.join("sub/trip.md")),
        "# Trip\n- [ ] [Hotels](../2026-09-30-lisbon-stays.md)\n"
    );
    assert_eq!(
        app.toast.as_deref(),
        Some("Renamed to Lisbon stays: 2 links follow")
    );
    assert_eq!(app.path.as_ref(), Some(&new), "the open note follows");
    assert!(app.editor.text().contains("title: Lisbon stays"));
    // Undo: the file, its title and every link back.
    let _ = app.update(Message::Undo);
    assert!(!new.exists());
    assert!(read(&old).contains("title: Lisbon hotels"));
    assert!(read(&root.join("plan.md")).contains("(2026-09-30-lisbon-hotels.md#rooms)"));
    assert_eq!(app.path.as_ref(), Some(&old));
    // F2 asks to rename the open note; refused over unsaved typing when
    // autosave is off (on, it writes the note first).
    app.settings.autosave = false;
    note(&mut app, NoteMessage::StartRename(None));
    assert!(matches!(&app.note_action, Some(NoteAction::Rename { path, .. }) if *path == old));
    app.editor.insert_text("typed ");
    note(&mut app, NoteMessage::RenameText("Elsewhere".into()));
    note(&mut app, NoteMessage::Confirm);
    assert!(old.exists());
    assert_eq!(app.error.as_deref(), Some("Save the open note first"));
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn delete_asks_then_undo_brings_the_note_back_open() {
    let (root, mut app) = vault("delete");
    let hotels = root.join("2026-09-30-lisbon-hotels.md");
    let plan = root.join("plan.md");
    note(&mut app, NoteMessage::StartDelete(plan.clone()));
    assert_eq!(app.note_action, Some(NoteAction::Delete(plan.clone())));
    note(&mut app, NoteMessage::Confirm);
    assert!(!plan.exists());
    assert_eq!(app.toast.as_deref(), Some("Deleted Plan"));
    let _ = app.update(Message::Undo);
    assert!(read(&plan).starts_with("# Plan"));
    // The open note: an empty one in its place, back open after Undo.
    note(&mut app, NoteMessage::StartDelete(hotels.clone()));
    note(&mut app, NoteMessage::Confirm);
    assert!(!hotels.exists());
    assert_eq!(app.path, None);
    assert_eq!(app.editor.text(), "");
    let _ = app.update(Message::Undo);
    assert_eq!(app.path.as_ref(), Some(&hotels));
    assert!(app.editor.text().contains("Shortlist."));
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn duplicate_opens_a_copy_and_copy_link_says_so_without_undo() {
    let (root, mut app) = vault("duplicate");
    let hotels = root.join("2026-09-30-lisbon-hotels.md");
    let copy = root.join("2026-09-30-lisbon-hotels-copy.md");
    note(&mut app, NoteMessage::Duplicate(hotels.clone()));
    assert!(read(&copy).contains("title: Lisbon hotels copy"));
    assert_eq!(app.path.as_ref(), Some(&copy));
    assert_eq!(
        app.toast.as_deref(),
        Some("Duplicated as Lisbon hotels copy")
    );
    let _ = app.update(Message::Undo);
    assert!(!copy.exists());
    assert_eq!(app.path.as_ref(), Some(&hotels), "back to the original");
    // A link from the open note, to paste; nothing to undo.
    let _ = app.update(Message::Opened(Some(root.join("sub/trip.md"))));
    note(&mut app, NoteMessage::CopyLink(hotels.clone()));
    assert_eq!(app.toast.as_deref(), Some("Copied a link to Lisbon hotels"));
    assert!(!app.toast_undo);
    // Escape closes a menu.
    note(&mut app, NoteMessage::Menu(hotels.clone()));
    let _ = app.update(Message::Escape);
    assert_eq!(app.note_menu, None);
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn a_note_outside_the_vault_moves_or_copies_in_with_front_matter_and_back() {
    use crate::into_vault::IntoVault;
    let base = std::env::temp_dir().join(format!("livemark-into-{}", std::process::id()));
    let (vault_dir, away) = (base.join("vault"), base.join("away"));
    std::fs::create_dir_all(&vault_dir).unwrap();
    std::fs::create_dir_all(&away).unwrap();
    let outside = away.join("notes.md");
    write(&outside, "# Meeting notes\nbody\n", 0);
    let mut app = App::open(Some(outside.clone()), None);
    let _ = app.update(Message::Vault(crate::sidebar::VaultMessage::Picked(Some(
        vault_dir.clone(),
    ))));
    let root = app.vault.as_ref().unwrap().root.clone();
    let inside = root.join("notes.md");
    assert!(app.outside_bar().is_some());
    let send = |app: &mut App, message| {
        let _ = app.update(Message::IntoVault(message));
        let _ = app.view();
    };
    // Unsaved typing first, autosave off: refused.
    app.settings.autosave = false;
    let end = app.editor.text().len();
    app.editor.select(end, end);
    app.editor.insert_text("x");
    send(&mut app, IntoVault::Move);
    assert!(outside.exists() && !inside.exists());
    app.saved = app.editor.version();
    let typed = app.editor.text().to_owned();
    std::fs::write(&outside, &typed).unwrap();
    // Moved: gone from where it was, in the vault with front matter.
    send(&mut app, IntoVault::Move);
    assert!(!outside.exists());
    let moved = read(&inside);
    assert!(moved.starts_with("---\ntitle: Meeting notes\ntags: []\ncreated: "));
    assert!(moved.ends_with(&typed));
    assert_eq!(app.path.as_ref(), Some(&inside));
    assert!(app.outside_bar().is_none());
    assert!(
        app.vault
            .as_ref()
            .unwrap()
            .notes
            .iter()
            .any(|n| n.path == inside)
    );
    let _ = app.update(Message::Undo);
    assert_eq!(read(&outside), typed, "back as it was");
    assert!(!inside.exists());
    assert_eq!(app.path.as_ref(), Some(&outside));
    // Copied: the original stays, the copy opens; Undo goes back to it.
    send(&mut app, IntoVault::Copy);
    assert!(outside.exists() && inside.exists());
    assert_eq!(app.path.as_ref(), Some(&inside));
    let _ = app.update(Message::Undo);
    assert!(!inside.exists());
    assert_eq!(app.path.as_ref(), Some(&outside));
    // The cross hides the bar for this file.
    send(&mut app, IntoVault::Dismiss);
    assert!(app.outside_bar().is_none());
    std::fs::remove_dir_all(&base).unwrap();
}
