//! The vault's flows, without a window (PLAN-004): opening one, new
//! notes, tags and links (the search field's in `search_tests.rs`).
use super::tests::write;
use super::{App, Message};

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
    let travel = crate::sidebar::Shown::Tag("travel".into());
    let _ = app.update(Message::Vault(VaultMessage::Show(travel.clone())));
    assert_eq!(app.shown, travel);
    let _ = app.view();
    let _ = app.update(Message::Vault(VaultMessage::Show(travel)));
    assert_eq!(app.shown, crate::sidebar::Shown::All);
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
    let start = std::time::Instant::now();
    let (hits, count) = crate::search::search(&vault, "words more");
    let everywhere = start.elapsed();
    assert_eq!((hits.len(), count), (200, 5000));
    let start = std::time::Instant::now();
    let (hits, _) = crate::search::search(&vault, "nowhere");
    let nowhere = start.elapsed();
    assert!(hits.is_empty());
    println!(
        "5,000 notes: open {opened:?}, refresh with no change {refreshed:?}, \
         search matching all {everywhere:?}, matching none {nowhere:?}"
    );
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

#[test]
fn a_tag_clicked_in_a_note_filters_the_sidebar() {
    use crate::search::SearchMessage;
    use crate::sidebar::VaultMessage;
    let dir = std::env::temp_dir().join(format!("livemark-tag-click-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    write(&dir.join("a.md"), "Plans #Travel\n", 0);
    let mut app = App::open(None, None);
    // Outside a vault, nothing to show.
    app.show_tag("travel");
    assert_eq!(app.shown, crate::sidebar::Shown::All);
    let _ = app.update(Message::Vault(VaultMessage::Picked(Some(dir.clone()))));
    // Matches listed in the sidebar (Ctrl+Enter) give way to the tag's notes.
    for message in [
        SearchMessage::Query("plans".into()),
        SearchMessage::Modifiers(iced::keyboard::Modifiers::COMMAND),
        SearchMessage::Choose(None),
    ] {
        let _ = app.update(Message::Search(message));
    }
    assert!(app.listing.is_some());
    app.show_tag("Travel");
    assert_eq!(app.shown, crate::sidebar::Shown::Tag("travel".into()));
    assert!(app.listing.is_none(), "back to the notes");
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn links_between_notes_are_offered_opened_and_listed_back() {
    use crate::sidebar::VaultMessage;
    let dir = std::env::temp_dir().join(format!("livemark-links-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("sub")).unwrap();
    write(
        &dir.join("2026-10-01-lisbon-hotels.md"),
        "---\ntitle: Lisbon [hotels]\ntags: [travel]\n---\n",
        0,
    );
    write(
        &dir.join("sub/plan.md"),
        "# Plan\nSee [hotels](../2026-10-01-lisbon-hotels.md).\n",
        10,
    );
    let mut app = App::open(None, None);
    let _ = app.update(Message::Vault(VaultMessage::Picked(Some(dir.clone()))));
    let root = app.vault.as_ref().unwrap().root.clone();
    let hotels = root.join("2026-10-01-lisbon-hotels.md");
    // Linked from: the plan points at the hotels.
    let back = app.vault.as_ref().unwrap().linked_from(&hotels);
    assert_eq!(
        back.iter().map(|n| n.title.as_str()).collect::<Vec<_>>(),
        ["Plan"]
    );
    let _ = app.update(Message::Opened(Some(hotels.clone())));
    assert!(app.linked_from().is_some());
    let _ = app.view();
    // Typing `[[` in the plan offers the hotels, as a link from there.
    let plan = root.join("sub/plan.md");
    let _ = app.update(Message::Opened(Some(plan.clone())));
    let end = app.editor.text().len();
    app.editor.select(end, end);
    app.editor.insert_text("[[lis");
    app.offer_choices();
    assert_eq!(
        app.completing.as_ref().map(|c| c.query.as_str()),
        Some("lis")
    );
    let choices = crate::links::link_choices(app.vault.as_ref().unwrap(), &plan, "lis");
    assert_eq!(
        choices[0].insert,
        "[Lisbon \\[hotels\\]](../2026-10-01-lisbon-hotels.md)"
    );
    // A click on a link to a note opens it here; web links do not.
    assert_eq!(
        app.note_link("../2026-10-01-lisbon-hotels.md"),
        Some(hotels.clone())
    );
    assert_eq!(app.note_link("https://x.org/a.md"), None);
    assert_eq!(app.note_link("missing.md"), None);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn the_sidebar_lists_tags_flat_and_filters_untagged_notes() {
    use crate::sidebar::{Shown, VaultMessage};
    let dir = std::env::temp_dir().join(format!("livemark-flat-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for i in 0..10 {
        write(
            &dir.join(format!("t{i}.md")),
            &format!("#tag{i} #common\n"),
            i,
        );
    }
    write(&dir.join("plain.md"), "no tags here\n", 20);
    let mut app = App::open(None, None);
    let _ = app.update(Message::Vault(VaultMessage::Picked(Some(dir.clone()))));
    let vault = app.vault.as_ref().unwrap();
    assert_eq!(
        vault.tags()[0],
        ("common".to_owned(), 10),
        "most used first"
    );
    let _ = app.view();
    let _ = app.update(Message::Vault(VaultMessage::AllTags));
    assert!(app.all_tags);
    let _ = app.view();
    let _ = app.update(Message::Vault(VaultMessage::Show(Shown::Untagged)));
    assert_eq!(app.shown, Shown::Untagged);
    let untagged: Vec<_> = vault_notes(&app);
    assert_eq!(untagged, ["plain"], "titled by its file name");
    // Clear: all again.
    let _ = app.update(Message::Vault(VaultMessage::Show(Shown::All)));
    assert_eq!(app.shown, Shown::All);
    std::fs::remove_dir_all(&dir).unwrap();
}

/// The titles of the notes the sidebar lists now.
fn vault_notes(app: &App) -> Vec<String> {
    let vault = app.vault.as_ref().unwrap();
    vault
        .notes
        .iter()
        .filter(|note| match &app.shown {
            crate::sidebar::Shown::All => true,
            crate::sidebar::Shown::Tag(tag) => note.tags.contains(tag),
            crate::sidebar::Shown::Untagged => note.tags.is_empty(),
        })
        .map(|note| note.title.clone())
        .collect()
}

#[test]
fn a_tag_edit_rewrites_the_vault_reloads_the_note_and_undoes() {
    use crate::sidebar::{Shown, VaultMessage};
    use crate::tags::Edit;
    let dir = std::env::temp_dir().join(format!("livemark-tag-edit-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let a = dir.join("a.md");
    let b = dir.join("b.md");
    write(&a, "---\ntags: [trips, work]\n---\nPack for #trips.\n", 0);
    write(&b, "---\ntags:\n  - travel\n  - trips\n---\n", 0);
    write(&dir.join("c.md"), "nothing to do with it\n", 0);
    let mut app = App::open(None, None);
    let _ = app.update(Message::Vault(VaultMessage::Picked(Some(dir.clone()))));
    let a = std::fs::canonicalize(&a).unwrap();
    let _ = app.update(Message::Opened(Some(a.clone())));
    let _ = app.update(Message::Vault(VaultMessage::Show(Shown::Tag(
        "trips".into(),
    ))));
    let merge = Edit::Rename {
        from: "trips".into(),
        to: "travel".into(),
    };
    assert_eq!(app.edit_tags(vec![merge.clone()]), Ok(2));
    assert_eq!(
        std::fs::read_to_string(&a).unwrap(),
        "---\ntags: [travel, work]\n---\nPack for #travel.\n"
    );
    assert_eq!(
        std::fs::read_to_string(&b).unwrap(),
        "---\ntags:\n  - travel\n---\n"
    );
    assert_eq!(
        app.editor.text(),
        "---\ntags: [travel, work]\n---\nPack for #travel.\n",
        "reloaded"
    );
    assert_eq!(app.shown, Shown::Tag("travel".into()), "the filter follows");
    assert!(
        app.vault
            .as_ref()
            .unwrap()
            .tags()
            .iter()
            .all(|(t, _)| t != "trips")
    );
    // Undo writes both back.
    assert_eq!(app.undo_tags(), Ok(0));
    assert_eq!(
        std::fs::read_to_string(&a).unwrap(),
        "---\ntags: [trips, work]\n---\nPack for #trips.\n"
    );
    assert_eq!(
        app.editor.text(),
        "---\ntags: [trips, work]\n---\nPack for #trips.\n"
    );
    // Unsaved typing in a note the edit touches: refused, nothing written.
    app.saved = u64::MAX;
    assert!(
        app.edit_tags(vec![merge])
            .unwrap_err()
            .contains("Save the open note")
    );
    assert_eq!(
        std::fs::read_to_string(&a).unwrap(),
        "---\ntags: [trips, work]\n---\nPack for #trips.\n"
    );
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn the_tag_menu_renames_merges_deletes_and_undoes() {
    use crate::sidebar::{Shown, VaultMessage};
    use crate::tag_actions::{TagAction, TagMessage};
    let dir = std::env::temp_dir().join(format!("livemark-tag-menu-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let a = dir.join("a.md");
    write(&a, "---\ntags: [trips]\n---\nGo. #old\n", 0);
    write(&dir.join("b.md"), "#travel notes\n", 0);
    let mut app = App::open(None, None);
    let _ = app.update(Message::Vault(VaultMessage::Picked(Some(dir.clone()))));
    let tag = |app: &mut App, m| {
        let _ = app.update(Message::Tag(m));
    };
    tag(&mut app, TagMessage::Menu("trips".into()));
    assert_eq!(app.tag_menu.as_deref(), Some("trips"));
    let _ = app.view();
    tag(&mut app, TagMessage::StartRename("trips".into()));
    tag(&mut app, TagMessage::RenameText("Travel".into()));
    let _ = app.view();
    tag(&mut app, TagMessage::Confirm);
    assert_eq!(
        std::fs::read_to_string(&a).unwrap(),
        "---\ntags: [travel]\n---\nGo. #old\n"
    );
    assert_eq!(
        app.toast.as_deref(),
        Some("Merged #trips into #travel: 1 note changed")
    );
    assert!(app.undo_toast().is_some());
    tag(&mut app, TagMessage::Undo);
    assert_eq!(
        std::fs::read_to_string(&a).unwrap(),
        "---\ntags: [trips]\n---\nGo. #old\n"
    );
    assert!(app.toast.is_none());
    // Delete: the word stays in the text.
    tag(&mut app, TagMessage::StartDelete("old".into()));
    assert_eq!(app.tag_action, Some(TagAction::Delete("old".into())));
    let _ = app.view();
    tag(&mut app, TagMessage::Confirm);
    assert_eq!(
        std::fs::read_to_string(&a).unwrap(),
        "---\ntags: [trips]\n---\nGo. old\n"
    );
    // F2 on the filtered tag, then Escape.
    let _ = app.update(Message::Vault(VaultMessage::Show(Shown::Tag(
        "travel".into(),
    ))));
    tag(&mut app, TagMessage::RenameShown);
    assert!(matches!(app.tag_action, Some(TagAction::Rename { ref tag, .. }) if tag == "travel"));
    tag(&mut app, TagMessage::Cancel);
    assert_eq!(app.tag_action, None);
    std::fs::remove_dir_all(&dir).unwrap();
}
