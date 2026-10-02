//! The app's file flows, without a window.
use std::time::{Duration, SystemTime};

use super::{App, Message};

/// Writes `text` to `path` with a modification time `secs` from now.
pub(crate) fn write(path: &std::path::Path, text: &str, secs: u64) {
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
    use crate::settings::Settings;
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
fn the_bar_says_whether_the_note_is_saved_and_the_sidebar_hides() {
    use crate::shell::SaveState;
    let dir = std::env::temp_dir().join(format!("livemark-shell-{}", std::process::id()));
    let vault = dir.join("vault");
    std::fs::create_dir_all(&vault).unwrap();
    let inside = vault.join("a.md");
    let outside = dir.join("b.md");
    write(&inside, "# A\n", 0);
    write(&outside, "# B\n", 0);
    let mut app = App::open(None, None);
    assert_eq!(app.save_state(), SaveState::New);
    app.editor.insert_text("typed");
    assert_eq!(app.save_state(), SaveState::Unsaved);
    let mut app = App::open(Some(outside.clone()), None);
    assert_eq!(app.save_state(), SaveState::Saved, "no vault: just saved");
    let _ = app.update(Message::Vault(crate::sidebar::VaultMessage::Picked(Some(
        vault.clone(),
    ))));
    assert_eq!(app.save_state(), SaveState::Outside);
    let _ = app.update(Message::Opened(Some(inside.clone())));
    assert_eq!(app.save_state(), SaveState::Saved);
    // Ctrl+\ hides the sidebar and shows it again; the menu opens over it.
    assert!(app.settings.sidebar);
    let _ = app.update(Message::Sidebar);
    assert!(!app.settings.sidebar);
    let _ = app.view();
    let _ = app.update(Message::Menu(true));
    let _ = app.view();
    let _ = app.update(Message::Sidebar);
    assert!(app.settings.sidebar);
    let _ = app.view();
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn appearance_picks_themes_follows_the_system_and_scales_the_interface() {
    use crate::appearance::AppearanceMessage as A;
    use crate::settings::Theme as Named;
    use iced::theme::Mode;
    let send = |app: &mut App, message| {
        let _ = app.update(Message::Appearance(message));
        let _ = app.view();
    };
    // A flag's theme until one is picked.
    let mut app = App::open(None, Some(iced::Theme::Dark));
    assert_eq!(app.current_theme(), Some(iced::Theme::Dark));
    send(&mut app, A::Open);
    assert!(app.appearance);
    send(&mut app, A::Theme(Named::KanagawaWave));
    assert_eq!(app.current_theme(), Some(iced::Theme::KanagawaWave));
    // System: the night pick when the system is dark, the day pick in light.
    send(
        &mut app,
        A::Pick {
            dark: true,
            theme: Named::CatppuccinMocha,
        },
    );
    assert_eq!(app.settings.theme, Named::System);
    assert_eq!(app.current_theme(), None, "until the system says");
    send(&mut app, A::System(Mode::Dark));
    assert_eq!(app.current_theme(), Some(iced::Theme::CatppuccinMocha));
    send(&mut app, A::System(Mode::Light));
    assert_eq!(app.current_theme(), Some(iced::Theme::Light));
    // The interface by tenths, from 50% to 200%.
    send(&mut app, A::Scale(1));
    send(&mut app, A::Scale(1));
    assert_eq!(app.settings.scale, 1.2);
    for _ in 0..20 {
        send(&mut app, A::Scale(1));
    }
    assert_eq!(app.settings.scale, 2.0);
    send(&mut app, A::Scale(0));
    assert_eq!(app.settings.scale, 1.0);
    // The note's text is left alone meanwhile; closing goes back to it.
    app.editor.insert_text("x");
    let before = app.editor.text().to_owned();
    send(&mut app, A::Close);
    assert!(!app.appearance);
    assert_eq!(app.editor.text(), before);
}

#[test]
fn the_sidebars_edge_drags_between_bounds_and_a_double_click_resets_it() {
    use crate::shell::Resize;
    let mut app = App::open(None, None);
    let drag = |app: &mut App, resize| {
        let _ = app.update(Message::Resize(resize));
        let _ = app.view();
    };
    assert_eq!(app.settings.sidebar_width, 260.0);
    drag(&mut app, Resize::Start);
    assert!(app.resizing, "a layer follows the pointer");
    drag(&mut app, Resize::To(350.0));
    assert_eq!(app.settings.sidebar_width, 350.0);
    drag(&mut app, Resize::To(2000.0));
    assert_eq!(app.settings.sidebar_width, 480.0);
    drag(&mut app, Resize::To(10.0));
    assert_eq!(app.settings.sidebar_width, 200.0);
    drag(&mut app, Resize::End);
    assert!(!app.resizing);
    drag(&mut app, Resize::Reset);
    assert_eq!(app.settings.sidebar_width, 260.0);
}
