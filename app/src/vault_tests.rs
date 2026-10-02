//! The vault's flows, without a window (PLAN-004): opening one, new
//! notes, quick open, search, tags and links.
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
fn quick_open_finds_notes_by_title_and_tag_and_opens_one() {
    use crate::quick::QuickMessage;
    use crate::sidebar::VaultMessage;
    let dir = std::env::temp_dir().join(format!("livemark-quick-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    write(
        &dir.join("a.md"),
        "---\ntitle: Lisbon hotels\ntags: [travel]\n---\n",
        0,
    );
    write(
        &dir.join("b.md"),
        "---\ntitle: Lisbon conference\ntags: [work]\n---\n",
        10,
    );
    write(&dir.join("c.md"), "# Standup\n", 20);
    let mut app = App::open(None, None);
    let _ = app.update(Message::Vault(VaultMessage::Picked(Some(dir.clone()))));
    let quick = |app: &mut App, message| {
        let _ = app.update(Message::Quick(message));
    };
    let titles = |app: &App| -> Vec<String> {
        app.quick
            .as_ref()
            .unwrap()
            .results
            .iter()
            .map(|r| r.0.clone())
            .collect()
    };
    quick(&mut app, QuickMessage::Open);
    assert_eq!(
        titles(&app),
        ["Standup", "Lisbon conference", "Lisbon hotels"],
        "recent first"
    );
    let _ = app.view();
    quick(&mut app, QuickMessage::Query("lis".into()));
    assert_eq!(titles(&app), ["Lisbon conference", "Lisbon hotels"]);
    quick(&mut app, QuickMessage::Query("lis #trav".into()));
    assert_eq!(titles(&app), ["Lisbon hotels"]);
    quick(&mut app, QuickMessage::Query("lis".into()));
    quick(&mut app, QuickMessage::Move(1));
    quick(&mut app, QuickMessage::Move(5));
    assert_eq!(app.quick.as_ref().unwrap().selected, 1, "stays on the last");
    quick(&mut app, QuickMessage::Choose(None));
    assert!(app.quick.is_none());
    assert_eq!(
        app.editor.text(),
        "---\ntitle: Lisbon hotels\ntags: [travel]\n---\n"
    );
    // Escape closes without opening anything.
    quick(&mut app, QuickMessage::Open);
    quick(&mut app, QuickMessage::Close);
    assert!(app.quick.is_none());
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn search_finds_notes_by_their_words_and_opens_at_the_match() {
    use crate::search::SearchMessage;
    use crate::sidebar::VaultMessage;
    let dir = std::env::temp_dir().join(format!("livemark-search-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    write(
        &dir.join("a.md"),
        "---\ntitle: Lisbon hotels\ntags: [travel]\n---\nThe hotel near Alfama.\nBook the HOTEL by Friday.\n",
        0,
    );
    write(
        &dir.join("b.md"),
        "# Conference\nThe hotel is paid by work. #work\n",
        10,
    );
    write(&dir.join("c.md"), "# Standup\nnothing here\n", 20);
    let mut app = App::open(None, None);
    let search = |app: &mut App, message| {
        let _ = app.update(Message::Search(message));
    };
    // Outside a vault there is nothing to search.
    search(&mut app, SearchMessage::Open);
    assert!(app.search.is_none());
    let _ = app.update(Message::Vault(VaultMessage::Picked(Some(dir.clone()))));
    search(&mut app, SearchMessage::Open);
    search(&mut app, SearchMessage::Query("hotel".into()));
    let found = |app: &App| -> Vec<(String, usize)> {
        let search = app.search.as_ref().unwrap();
        search
            .hits
            .iter()
            .map(|h| (h.title.clone(), h.lines.len()))
            .collect()
    };
    // The title in its front matter is a line too.
    assert_eq!(
        found(&app),
        [
            ("Conference".to_owned(), 1),
            ("Lisbon hotels".to_owned(), 3)
        ]
    );
    let _ = app.view();
    // Every word, and a tag.
    search(&mut app, SearchMessage::Query("hotel friday".into()));
    assert_eq!(found(&app), [("Lisbon hotels".to_owned(), 3)]);
    search(&mut app, SearchMessage::Query("hotel #wo".into()));
    assert_eq!(found(&app), [("Conference".to_owned(), 1)]);
    // Enter: the first match, selected.
    search(&mut app, SearchMessage::Query("friday".into()));
    search(&mut app, SearchMessage::First);
    assert_eq!(
        app.editor.text()[app.editor.selection().range()].to_owned(),
        "Friday"
    );
    search(&mut app, SearchMessage::Close);
    assert!(app.search.is_none());
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_tag_clicked_in_a_note_filters_the_sidebar() {
    use crate::sidebar::VaultMessage;
    let dir = std::env::temp_dir().join(format!("livemark-tag-click-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    write(&dir.join("a.md"), "Plans #Travel\n", 0);
    let mut app = App::open(None, None);
    // Outside a vault, nothing to show.
    app.show_tag("travel");
    assert_eq!(app.tag, None);
    let _ = app.update(Message::Vault(VaultMessage::Picked(Some(dir.clone()))));
    let _ = app.update(Message::Search(crate::search::SearchMessage::Open));
    app.show_tag("Travel");
    assert_eq!(app.tag.as_deref(), Some("travel"));
    assert!(app.search.is_none(), "back to the notes");
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
