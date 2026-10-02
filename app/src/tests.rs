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
