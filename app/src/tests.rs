//! The app's file flows, without a window.
use std::time::{Duration, SystemTime};

use super::{App, Message};

/// Writes `text` to `path` with a modification time `secs` from now.
fn write(path: &std::path::Path, text: &str, secs: u64) {
    std::fs::write(path, text).unwrap();
    let file = std::fs::File::options().write(true).open(path).unwrap();
    file.set_modified(SystemTime::now() + Duration::from_secs(secs))
        .unwrap();
}

#[test]
fn a_file_changed_on_disk_loads_on_focus_or_asks_with_edits() {
    let dir = std::env::temp_dir().join(format!("livemark-focus-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("note.md");
    write(&path, "one\n", 0);
    let mut app = App::open(Some(path.clone()), None);
    let _ = app.update(Message::Focused);
    assert_eq!(app.editor.text(), "one\n", "unchanged");
    write(&path, "two\n", 10);
    let _ = app.update(Message::Focused);
    assert_eq!(app.editor.text(), "two\n", "loaded again");
    assert!(!app.unsaved());
    // With unsaved changes it asks; keeping them stops the asking.
    app.saved = u64::MAX;
    write(&path, "three\n", 20);
    let _ = app.update(Message::Focused);
    assert!(app.changed);
    assert_eq!(app.editor.text(), "two\n");
    let _ = app.update(Message::Reload(false));
    let _ = app.update(Message::Focused);
    assert!(!app.changed, "kept");
    write(&path, "four\n", 30);
    let _ = app.update(Message::Focused);
    let _ = app.update(Message::Reload(true));
    assert_eq!(app.editor.text(), "four\n", "loaded, edits dropped");
    // Saving over a change made on disk asks first; keeping mine lets
    // the next save through.
    write(&path, "five\n", 40);
    let _ = app.update(Message::Save { choose: false });
    assert!(app.changed, "asked, not saved");
    let _ = app.update(Message::Reload(false));
    let _ = app.update(Message::Save { choose: false });
    assert!(!app.changed, "saving");
    // A file picked while there is unsaved typing waits for an answer;
    // one picked after discarding loads.
    let other = dir.join("other.md");
    write(&other, "other\n", 0);
    app.saved = u64::MAX;
    let _ = app.update(Message::Opened(Some(other.clone())));
    assert!(matches!(app.pending, Some(super::After::Load(_))));
    assert_ne!(app.editor.text(), "other\n");
    let _ = app.update(Message::Unsaved(None));
    app.discarded = Some(app.editor.version());
    let _ = app.update(Message::Opened(Some(other.clone())));
    assert_eq!(app.editor.text(), "other\n");
    std::fs::remove_file(&other).unwrap();
    app.path = Some(path.clone());
    // Ctrl+N with unsaved changes asks first; discarding starts afresh.
    app.saved = u64::MAX;
    let _ = app.update(Message::New);
    assert_eq!(app.editor.text(), "other\n", "asked first");
    let _ = app.update(Message::Unsaved(Some(false)));
    assert_eq!((app.editor.text(), app.path.is_none()), ("", true));
    std::fs::remove_file(&path).unwrap();
    std::fs::remove_dir(&dir).unwrap();
}

#[test]
fn a_new_path_a_reload_and_loading_after_a_close() {
    use livemark::widget::Mode;
    let dir = std::env::temp_dir().join(format!("livemark-new-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    // A path that does not exist yet opens empty, kept for Ctrl+S.
    let path = dir.join("new.md");
    let app = App::open(Some(path.clone()), None);
    assert_eq!(app.path.as_deref(), Some(path.as_path()));
    assert!(app.error.is_none());
    // Loading it again from disk keeps source mode.
    write(&path, "one\n", 0);
    let mut app = App::open(Some(path.clone()), None);
    app.editor.set_mode(Mode::Source);
    write(&path, "two\n", 10);
    let _ = app.update(Message::Focused);
    assert_eq!(
        (app.editor.text(), app.editor.mode()),
        ("two\n", Mode::Source)
    );
    // Closing with unsaved text, saving, the file changed on disk, "Load
    // it": nothing is unsaved, so nothing waits on it.
    app.saved = u64::MAX;
    write(&path, "three\n", 20);
    let _ = app.update(Message::CloseRequested);
    let _ = app.update(Message::Unsaved(Some(true)));
    assert!(app.changed, "asked about the disk first");
    let _ = app.update(Message::Reload(true));
    assert!(app.pending.is_none() && !app.unsaved());
    std::fs::remove_file(&path).unwrap();
    std::fs::remove_dir(&dir).unwrap();
}

#[test]
fn zoom_keys_step_by_a_tenth_and_reset() {
    let mut app = App::open(None, None);
    for _ in 0..3 {
        let _ = app.update(Message::Zoom(1));
    }
    assert!((app.editor.zoom() - 1.3).abs() < 1e-6);
    let _ = app.update(Message::Zoom(-1));
    assert!((app.editor.zoom() - 1.2).abs() < 1e-6);
    let _ = app.update(Message::Zoom(0));
    assert_eq!(app.editor.zoom(), 1.0);
    for _ in 0..20 {
        let _ = app.update(Message::Zoom(-1));
    }
    assert_eq!(app.editor.zoom(), 0.5, "no smaller than half");
}

#[test]
fn pictures_next_to_the_note_are_handed_to_the_editor() {
    let dir = std::env::temp_dir().join(format!("livemark-images-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("assets")).unwrap();
    // The editor decodes; here only which bytes are read matters.
    let png = b"picture bytes".to_vec();
    std::fs::write(dir.join("assets").join("my pic.png"), &png).unwrap();
    let note = dir.join("note.md");
    write(
        &note,
        "![a](assets/my%20pic.png)\n![web](https://example.com/x.png)\n",
        0,
    );
    let app = App::open(Some(note.clone()), None);
    assert!(app.tried.contains("assets/my%20pic.png"));
    assert_eq!(
        crate::file::image_bytes(&note, "assets/my%20pic.png"),
        Some(png)
    );
    assert_eq!(
        crate::file::image_bytes(&note, "https://example.com/x.png"),
        None,
        "no network"
    );
    assert_eq!(
        crate::file::image_bytes(&note, "data:image/png;base64,AA"),
        None
    );
    std::fs::remove_file(dir.join("assets").join("my pic.png")).unwrap();
    std::fs::remove_dir(dir.join("assets")).unwrap();
    std::fs::remove_file(&note).unwrap();
    std::fs::remove_dir(&dir).unwrap();
}

#[test]
fn settings_remember_the_zoom_the_window_and_recent_notes() {
    use crate::settings::Settings;
    let dir = std::env::temp_dir().join(format!("livemark-settings-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("config").join("settings");
    let note = dir.join("note.md");
    write(&note, "# Note\n", 0);
    let mut app =
        App::open(Some(note.clone()), None).with_settings(Settings::default(), Some(file.clone()));
    let _ = app.update(Message::Zoom(1));
    let _ = app.update(Message::Resized(iced::Size::new(900.0, 700.0)));
    let _ = app.update(Message::CloseRequested);
    // The next start finds them.
    let saved = Settings::load(&file);
    assert_eq!(saved.zoom, 1.1);
    assert_eq!(saved.window, Some((900.0, 700.0)));
    assert_eq!(saved.recent, [std::fs::canonicalize(&note).unwrap()]);
    let again = App::open(None, None).with_settings(saved, None);
    assert_eq!(again.editor.zoom(), 1.1);
    std::fs::remove_file(&file).unwrap();
    std::fs::remove_dir(dir.join("config")).unwrap();
    std::fs::remove_file(&note).unwrap();
    std::fs::remove_dir(&dir).unwrap();
}

#[test]
fn a_pasted_picture_is_kept_next_to_the_note_and_linked() {
    let dir = std::env::temp_dir().join(format!("livemark-picture-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let note = dir.join("meeting.md");
    write(&note, "Notes\n", 0);
    let mut app = App::open(Some(note.clone()), None);
    app.editor.select(6, 6);
    let _ = app.paste_picture(b"png bytes".to_vec());
    assert_eq!(app.editor.text(), "Notes\n![](assets/meeting-1.png)");
    assert_eq!(
        std::fs::read(dir.join("assets/meeting-1.png")).unwrap(),
        b"png bytes"
    );
    // A dropped picture is linked by its path from the note.
    let _ = app.dropped(dir.join("assets").join("meeting-1.png"));
    assert!(
        app.editor
            .text()
            .ends_with("![](assets/meeting-1.png)![](assets/meeting-1.png)")
    );
    // In a note not saved yet, the picture waits for a place.
    let mut new = App::open(None, None);
    let _ = new.paste_picture(b"later".to_vec());
    assert!(new.waiting_picture.is_some() && new.editor.text().is_empty());
    std::fs::remove_file(dir.join("assets/meeting-1.png")).unwrap();
    std::fs::remove_dir(dir.join("assets")).unwrap();
    std::fs::remove_file(&note).unwrap();
    std::fs::remove_dir(&dir).unwrap();
}

#[test]
fn the_file_menu_opens_recent_notes_and_sets_the_theme() {
    use crate::settings::{Settings, Theme};
    let dir = std::env::temp_dir().join(format!("livemark-menu-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let (a, b, gone) = (dir.join("a.md"), dir.join("b.md"), dir.join("gone.md"));
    write(&a, "a\n", 0);
    write(&b, "b\n", 0);
    let settings = Settings {
        recent: vec![a.clone(), gone.clone(), b.clone()],
        ..Settings::default()
    };
    let mut app = App::open(None, None).with_settings(settings, None);
    assert_eq!(
        app.settings.recent,
        [a.clone(), b.clone()],
        "missing ones dropped"
    );
    let _ = app.update(Message::Menu(true));
    let _ = app.view();
    std::fs::remove_file(&b).unwrap();
    let _ = app.update(Message::Menu(true));
    assert_eq!(
        app.settings.recent,
        std::slice::from_ref(&a),
        "dropped when the menu opens"
    );
    // A pick closes the menu and opens the note.
    let _ = app.update(Message::Recent(a.clone()));
    assert!(!app.menu);
    assert_eq!(app.editor.text(), "a\n");
    // One gone since the menu opened: said, and forgotten.
    app.settings.recent.push(gone.clone());
    let _ = app.update(Message::Recent(gone.clone()));
    assert!(app.error.as_deref().is_some_and(|e| e.contains("gone.md")));
    assert!(!app.settings.recent.contains(&gone));
    // With unsaved typing, it asks first.
    app.saved = u64::MAX;
    let _ = app.update(Message::Recent(a.clone()));
    assert!(matches!(app.pending, Some(super::After::Load(_))));
    // The theme is shown and remembered.
    let _ = app.update(Message::Theme(Theme::Dark));
    assert_eq!(app.theme, Some(iced::Theme::Dark));
    assert_eq!(app.settings.theme, Theme::Dark);
    let _ = app.update(Message::Theme(Theme::System));
    assert_eq!(app.theme, None);
    std::fs::remove_file(&a).unwrap();
    std::fs::remove_dir(&dir).unwrap();
}

#[test]
fn another_note_keeps_the_zoom_the_mode_and_the_divider() {
    use livemark::widget::Mode;
    let dir = std::env::temp_dir().join(format!("livemark-keep-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let other = dir.join("other.md");
    write(&other, "other\n", 0);
    let mut app = App::open(None, None);
    app.editor.set_zoom(1.5);
    app.editor.set_mode(Mode::Split);
    app.editor.set_split_ratio(0.3);
    let _ = app.update(Message::Opened(Some(other.clone())));
    assert_eq!(app.editor.text(), "other\n");
    assert_eq!(
        (
            app.editor.zoom(),
            app.editor.mode(),
            app.editor.split_ratio()
        ),
        (1.5, Mode::Split, 0.3)
    );
    let _ = app.update(Message::New);
    assert_eq!(app.editor.zoom(), 1.5, "a new note too");
    // The divider is kept with the settings.
    let _ = app.update(Message::Zoom(1));
    assert_eq!(app.settings.split, 0.3);
    std::fs::remove_file(&other).unwrap();
    std::fs::remove_dir(&dir).unwrap();
}

#[test]
fn a_vault_opens_lists_its_notes_and_filters_by_tag() {
    use crate::sidebar::VaultMessage;
    let dir = std::env::temp_dir().join(format!("livemark-app-vault-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    write(
        &dir.join("2026-10-01-lisbon.md"),
        "---\ntags: [travel]\n---\n# Lisbon\n",
        0,
    );
    write(&dir.join("2026-10-02-standup.md"), "# Standup\n#work\n", 10);
    let mut app = App::open(None, None);
    let _ = app.update(Message::Vault(VaultMessage::Picked(Some(dir.clone()))));
    let vault = app.vault.as_ref().expect("opened");
    assert_eq!(vault.notes[0].title, "Standup", "most recent first");
    assert_eq!(
        app.settings.vault,
        Some(std::fs::canonicalize(&dir).unwrap())
    );
    assert!(app.sidebar().is_some());
    let _ = app.view();
    // A tag filters; the same tag again shows all.
    let _ = app.update(Message::Vault(VaultMessage::Tag(Some("travel".into()))));
    assert_eq!(app.tag.as_deref(), Some("travel"));
    let _ = app.view();
    let _ = app.update(Message::Vault(VaultMessage::Tag(Some("travel".into()))));
    assert_eq!(app.tag, None);
    // A note made elsewhere shows up when the window comes back.
    write(&dir.join("2026-10-03-agent.md"), "# From an agent\n", 20);
    let _ = app.update(Message::Focused);
    assert_eq!(app.vault.as_ref().unwrap().notes[0].title, "From an agent");
    // Opening one from the list is an ordinary open.
    let lisbon = std::fs::canonicalize(dir.join("2026-10-01-lisbon.md")).unwrap();
    let _ = app.update(Message::Opened(Some(lisbon.clone())));
    assert_eq!(app.path.as_ref(), Some(&lisbon));
    // Not a folder: said, and nothing changes.
    let _ = app.update(Message::Vault(VaultMessage::Picked(Some(
        dir.join("2026-10-01-lisbon.md"),
    ))));
    assert!(app.error.is_some());
    assert!(app.vault.is_some());
    std::fs::remove_dir_all(&dir).unwrap();
}

/// A vault of 5,000 notes read in full; run with `cargo test --release -p
/// livemark-app -- --ignored` (PLAN-004 slice 86).
#[test]
#[ignore = "timing, release only"]
fn five_thousand_notes_are_read_quickly() {
    let dir = std::env::temp_dir().join(format!("livemark-vault-5k-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let body = "Some text with **bold** and a [link](x.md), #tag and more words.\n".repeat(30);
    for i in 0..5000 {
        let text = format!(
            "---\ntitle: Note {i}\ntags: [t{}]\n---\n# Note {i}\n{body}",
            i % 20
        );
        std::fs::write(dir.join(format!("2026-01-01-note-{i}.md")), text).unwrap();
    }
    let start = std::time::Instant::now();
    let mut vault = crate::vault::Vault::open(&dir).unwrap();
    let opened = start.elapsed();
    let start = std::time::Instant::now();
    vault.refresh();
    let refreshed = start.elapsed();
    println!("5,000 notes: open {opened:?}, refresh with no change {refreshed:?}");
    assert_eq!(vault.notes.len(), 5000);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn ctrl_n_in_a_vault_names_a_note_and_opens_it_ready_to_type() {
    use crate::sidebar::VaultMessage;
    let dir = std::env::temp_dir().join(format!("livemark-new-note-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut app = App::open(None, None);
    let _ = app.update(Message::Vault(VaultMessage::Picked(Some(dir.clone()))));
    let _ = app.update(Message::New);
    assert_eq!(app.naming.as_deref(), Some(""), "asks for a title");
    let _ = app.view();
    let _ = app.update(Message::Vault(VaultMessage::Title("Lisbon hotels".into())));
    let _ = app.update(Message::Vault(VaultMessage::Create));
    let path = app.path.clone().expect("opened");
    let name = path.file_name().unwrap().to_string_lossy().into_owned();
    assert!(
        name.ends_with("-lisbon-hotels.md") && name.len() == "2026-10-02-lisbon-hotels.md".len()
    );
    assert!(
        app.editor
            .text()
            .starts_with("---\ntitle: Lisbon hotels\ntags: []\ncreated: ")
    );
    assert_eq!(
        app.editor.selection().head,
        app.editor.text().len(),
        "caret after it"
    );
    assert_eq!(app.vault.as_ref().unwrap().notes[0].title, "Lisbon hotels");
    // Cancelling makes nothing; outside a vault Ctrl+N is still a blank note.
    let _ = app.update(Message::New);
    let _ = app.update(Message::Vault(VaultMessage::CancelNew));
    assert_eq!(app.naming, None);
    assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
    std::fs::remove_dir_all(&dir).unwrap();
}
