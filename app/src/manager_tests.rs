//! The tag manager's flows, without a window (PLAN-005).
use super::manager::{Column, ManagerMessage, sorted};
use super::tests::write;
use super::{App, Message};

#[test]
fn the_manager_sorts_suggests_and_merges_or_deletes_several_with_one_undo() {
    use crate::sidebar::VaultMessage;
    let dir = std::env::temp_dir().join(format!("livemark-manager-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let (a, b, c) = (dir.join("a.md"), dir.join("b.md"), dir.join("c.md"));
    let a_text = "---\ntags: [ops, work]\ncreated: 2026-09-01\n---\nOn #internal duty.\n";
    let b_text = "---\ntags: [internal]\ncreated: 2026-09-20\n---\nb\n";
    write(&a, a_text, 0);
    write(&b, b_text, 0);
    write(&c, "---\ntags: [trip, trips]\n---\nc\n", 0);
    write(&dir.join("d.md"), "#trips again\n", 0);
    write(&dir.join("e.md"), "plain\n", 0);
    let mut app = App::open(None, None);
    let _ = app.update(Message::Vault(VaultMessage::Picked(Some(dir.clone()))));
    let send = |app: &mut App, m| {
        let _ = app.update(Message::Manager(m));
        let _ = app.view();
    };
    assert_eq!(app.suggestion(), Some(("trip".into(), "trips".into())));
    send(&mut app, ManagerMessage::Open(None));
    let order = |app: &App| -> Vec<String> {
        let vault = app.vault.as_ref().unwrap();
        sorted(vault, vault.tags(), app.manager.as_ref().unwrap())
            .into_iter()
            .map(|(tag, _, used)| format!("{tag} {used}"))
            .collect()
    };
    send(&mut app, ManagerMessage::Sort(Column::Name));
    assert_eq!(
        order(&app),
        [
            "internal 2026-09-20",
            "ops 2026-09-01",
            "trip ",
            "trips ",
            "work 2026-09-01"
        ]
    );
    send(&mut app, ManagerMessage::Sort(Column::Used));
    assert_eq!(order(&app)[0], "internal 2026-09-20");
    // The suggestion taken: the less used into the more used.
    send(
        &mut app,
        ManagerMessage::Accept("trip".into(), "trips".into()),
    );
    assert_eq!(
        std::fs::read_to_string(&c).unwrap(),
        "---\ntags: [trips]\n---\nc\n"
    );
    assert_eq!(app.suggestion(), None);
    // Three merged into a tag no note has yet, one Undo for both notes.
    for tag in ["ops", "work", "internal"] {
        send(&mut app, ManagerMessage::Select(tag.into(), true));
    }
    send(&mut app, ManagerMessage::Merging);
    send(&mut app, ManagerMessage::Another("Duty".into()));
    send(&mut app, ManagerMessage::MergeInto("Duty".into()));
    assert_eq!(
        std::fs::read_to_string(&a).unwrap(),
        "---\ntags: [duty]\ncreated: 2026-09-01\n---\nOn #duty duty.\n"
    );
    assert_eq!(
        std::fs::read_to_string(&b).unwrap(),
        "---\ntags: [duty]\ncreated: 2026-09-20\n---\nb\n"
    );
    assert_eq!(
        app.toast.as_deref(),
        Some("Merged #internal, #ops and #work into #duty: 2 notes changed")
    );
    assert!(app.manager.as_ref().unwrap().selected.is_empty());
    let _ = app.update(Message::Undo);
    assert_eq!(std::fs::read_to_string(&a).unwrap(), a_text);
    assert_eq!(std::fs::read_to_string(&b).unwrap(), b_text);
    // Deleting the selected, after the question.
    send(&mut app, ManagerMessage::Select("ops".into(), true));
    send(&mut app, ManagerMessage::Delete);
    assert!(app.manager.as_ref().unwrap().deleting);
    send(&mut app, ManagerMessage::ConfirmDelete);
    assert_eq!(
        std::fs::read_to_string(&a).unwrap(),
        "---\ntags: [work]\ncreated: 2026-09-01\n---\nOn #internal duty.\n"
    );
    assert_eq!(app.toast.as_deref(), Some("Deleted #ops: 1 note changed"));
    // A note picked in the sidebar goes back to the note.
    let _ = app.update(Message::Opened(Some(b.clone())));
    assert!(app.manager.is_none());
    std::fs::remove_dir_all(&dir).unwrap();
}
