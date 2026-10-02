//! Right-click menus and changing vaults, without a window (PLAN-006).
use super::context::{ContextMessage, Target};
use super::note_actions::{NoteAction, NoteMessage};
use super::sidebar::{Shown, VaultMessage};
use super::tag_actions::{TagAction, TagMessage};
use super::tests::write;
use super::{App, Message};

fn two_vaults(name: &str) -> (std::path::PathBuf, std::path::PathBuf, std::path::PathBuf) {
    let base = std::env::temp_dir().join(format!("livemark-{name}-{}", std::process::id()));
    let (a, b) = (base.join("a"), base.join("b"));
    std::fs::create_dir_all(&a).unwrap();
    std::fs::create_dir_all(&b).unwrap();
    write(&a.join("one.md"), "# One\nIn A. #work\n", 0);
    write(&a.join("two.md"), "# Two\nAlso A.\n", 10);
    write(&b.join("three.md"), "# Three\nIn B.\n", 0);
    (base, a, b)
}

fn press(app: &mut App, x: f32, y: f32, right: bool) {
    let _ = app.update(Message::Pressed {
        at: iced::Point::new(x, y),
        right,
    });
}

fn context(app: &mut App, message: ContextMessage) {
    let _ = app.update(Message::Context(message));
    let _ = app.view();
}

#[test]
fn a_menu_opens_where_the_pointer_pressed_and_closes_on_any_other_press() {
    let (base, a, _) = two_vaults("menus");
    let mut app = App::open(None, None);
    let _ = app.update(Message::Vault(VaultMessage::Picked(Some(a.clone()))));
    let root = app.vault.as_ref().unwrap().root.clone();
    let one = root.join("one.md");
    // A right press on a note: its menu, at the pointer.
    press(&mut app, 120.0, 300.0, true);
    context(&mut app, ContextMessage::Open(Target::Note(one.clone())));
    let menu = app.context.clone().unwrap();
    assert_eq!((menu.at.x, menu.at.y), (120.0, 300.0));
    // A right press elsewhere closes it; on a tag, that tag's opens.
    press(&mut app, 60.0, 200.0, true);
    assert!(app.context.is_none());
    context(&mut app, ContextMessage::Open(Target::Tag("work".into())));
    assert_eq!(
        app.context.as_ref().unwrap().target,
        Target::Tag("work".into())
    );
    // A left press elsewhere (the catcher) closes it; so does Escape.
    context(&mut app, ContextMessage::Close);
    assert!(app.context.is_none());
    context(&mut app, ContextMessage::Open(Target::Note(one.clone())));
    let _ = app.update(Message::Escape);
    assert!(app.context.is_none());
    // An item picked: the menu closes and the item acts.
    context(&mut app, ContextMessage::Open(Target::Note(one.clone())));
    context(
        &mut app,
        ContextMessage::Pick(Box::new(Message::Note(NoteMessage::StartRename(Some(
            one.clone(),
        ))))),
    );
    assert!(app.context.is_none());
    assert!(matches!(&app.note_action, Some(NoteAction::Rename { path, .. }) if *path == one));
    // Opening another menu takes back a question left open.
    context(&mut app, ContextMessage::Open(Target::Tag("work".into())));
    assert!(app.note_action.is_none());
    context(
        &mut app,
        ContextMessage::Pick(Box::new(Message::Tag(TagMessage::StartDelete(
            "work".into(),
        )))),
    );
    assert_eq!(app.tag_action, Some(TagAction::Delete("work".into())));
    std::fs::remove_dir_all(&base).unwrap();
}

#[test]
fn the_texts_menu_cuts_and_selects_all() {
    let mut app = App::open(None, None);
    app.editor.insert_text("keep cut");
    app.editor.select(4, 8);
    context(&mut app, ContextMessage::Open(Target::Text));
    context(&mut app, ContextMessage::Cut);
    assert_eq!(app.editor.text(), "keep");
    assert!(app.context.is_none());
    context(&mut app, ContextMessage::Open(Target::Text));
    context(&mut app, ContextMessage::SelectAll);
    assert_eq!(app.editor.selection().range(), 0..4);
}

#[test]
fn a_vault_comes_in_fresh_with_its_note_and_is_one_press_away_after() {
    let (base, a, b) = two_vaults("switch");
    let mut app = App::open(None, None);
    let _ = app.update(Message::Vault(VaultMessage::Picked(Some(a.clone()))));
    let (root_a, one) = {
        let root = app.vault.as_ref().unwrap().root.clone();
        (root.clone(), root.join("one.md"))
    };
    assert_eq!(
        app.path.as_ref(),
        Some(&root_a.join("two.md")),
        "its newest"
    );
    let _ = app.update(Message::Opened(Some(one.clone())));
    // Things of vault A about: a filter, the tag manager, an Undo.
    let _ = app.update(Message::Vault(VaultMessage::Show(Shown::Tag(
        "work".into(),
    ))));
    let _ = app.update(Message::Manager(crate::manager::ManagerMessage::Open(None)));
    app.undo = Some(crate::undo::Undo::default());
    let _ = app.update(Message::Vault(VaultMessage::Picked(Some(b.clone()))));
    let root_b = app.vault.as_ref().unwrap().root.clone();
    assert_eq!(app.path.as_ref(), Some(&root_b.join("three.md")));
    assert_eq!(app.shown, Shown::All);
    assert!(app.manager.is_none() && app.undo.is_none());
    assert_eq!(app.settings.vaults, [root_b.clone(), root_a.clone()]);
    let _ = app.view();
    // Back to A from the menu: the note last open there.
    let _ = app.update(Message::Vault(VaultMessage::Picked(Some(root_a.clone()))));
    assert_eq!(app.path.as_ref(), Some(&one));
    // A recent note of B opened from A: B comes in with it.
    let _ = app.update(Message::Recent(root_b.join("three.md")));
    assert_eq!(app.vault.as_ref().unwrap().root, root_b);
    assert_eq!(app.path.as_ref(), Some(&root_b.join("three.md")));
    // Closing the vault keeps the note open, a plain file.
    let _ = app.update(Message::Vault(VaultMessage::Close));
    assert!(app.vault.is_none());
    assert_eq!(app.path.as_ref(), Some(&root_b.join("three.md")));
    // A vault gone since: said, and forgotten.
    std::fs::remove_dir_all(&root_a).unwrap();
    let _ = app.update(Message::Vault(VaultMessage::Picked(Some(root_a.clone()))));
    assert!(app.error.is_some());
    assert!(!app.settings.vaults.contains(&root_a));
    std::fs::remove_dir_all(&base).unwrap();
}
