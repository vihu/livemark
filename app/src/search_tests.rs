//! The search field's flows, without a window (PLAN-005).
use super::tests::write;
use super::{App, Message};

#[test]
fn the_search_field_finds_titles_lines_and_tags_and_opens_them() {
    use crate::search::{Found, SearchMessage};
    use crate::sidebar::{Shown, VaultMessage};
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
    write(
        &dir.join("c.md"),
        "# Standup\nnothing here, Lisbon later\n",
        20,
    );
    let mut app = App::open(None, None);
    let _ = app.update(Message::Vault(VaultMessage::Picked(Some(dir.clone()))));
    let search = |app: &mut App, message| {
        let _ = app.update(Message::Search(message));
        let _ = app.view();
    };
    // Each result by its group's first letter and its title or tag.
    let found = |app: &App| -> Vec<String> {
        app.search
            .as_ref()
            .unwrap()
            .found
            .iter()
            .map(|found| match found {
                Found::Note(title, ..) => format!("N {title}"),
                Found::Line { title, number, .. } => format!("L {title} {number}"),
                Found::Tag(tag, count) => format!("T {tag} {count}"),
            })
            .collect()
    };
    search(&mut app, SearchMessage::Open);
    assert_eq!(
        found(&app),
        ["N Standup", "N Conference", "N Lisbon hotels"],
        "nothing typed: the recent notes"
    );
    // A title, and the text of the other notes.
    search(&mut app, SearchMessage::Query("lisbon".into()));
    assert_eq!(found(&app), ["N Lisbon hotels", "L Standup 2"]);
    search(&mut app, SearchMessage::Query("hotel".into()));
    assert_eq!(found(&app), ["N Lisbon hotels", "L Conference 2"]);
    // A tag narrows; tags starting so are offered.
    search(&mut app, SearchMessage::Query("hotel #wo".into()));
    assert_eq!(found(&app), ["L Conference 2", "T work 1"]);
    search(&mut app, SearchMessage::Query("#tra".into()));
    assert_eq!(found(&app), ["N Lisbon hotels", "T travel 1"]);
    // Down to the tag, Enter: the sidebar shows its notes.
    search(&mut app, SearchMessage::Move(1));
    search(&mut app, SearchMessage::Move(5));
    assert_eq!(
        app.search.as_ref().unwrap().selected,
        1,
        "stays on the last"
    );
    search(&mut app, SearchMessage::Choose(None));
    assert!(app.search.is_none());
    assert_eq!(app.shown, Shown::Tag("travel".into()));
    // A line opens its note with the match selected.
    search(&mut app, SearchMessage::Query("friday".into()));
    assert_eq!(found(&app), ["L Lisbon hotels 6"]);
    search(&mut app, SearchMessage::Choose(None));
    assert_eq!(
        app.editor.text()[app.editor.selection().range()].to_owned(),
        "Friday"
    );
    // Escape closes without opening anything.
    search(&mut app, SearchMessage::Open);
    search(&mut app, SearchMessage::Close);
    assert!(app.search.is_none());
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn ctrl_enter_lists_every_match_in_the_sidebar_until_cleared() {
    use crate::search::SearchMessage;
    use crate::sidebar::VaultMessage;
    let dir = std::env::temp_dir().join(format!("livemark-listing-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    write(&dir.join("a.md"), "# One\nThe hotel.\nA hotel again.\n", 0);
    write(&dir.join("b.md"), "# Two\nNo hotel here? A hotel.\n", 10);
    write(&dir.join("c.md"), "# Three\nnothing\n", 20);
    let mut app = App::open(None, None);
    let search = |app: &mut App, message| {
        let _ = app.update(Message::Search(message));
        let _ = app.view();
    };
    // Outside a vault the field finds recent files, and lists nothing.
    search(&mut app, SearchMessage::Query("x".into()));
    search(
        &mut app,
        SearchMessage::Modifiers(iced::keyboard::Modifiers::COMMAND),
    );
    search(&mut app, SearchMessage::Choose(None));
    assert!(app.listing.is_none());
    search(&mut app, SearchMessage::Close);
    let _ = app.update(Message::Vault(VaultMessage::Picked(Some(dir.clone()))));
    search(&mut app, SearchMessage::Open);
    search(&mut app, SearchMessage::Query("hotel".into()));
    search(
        &mut app,
        SearchMessage::Modifiers(iced::keyboard::Modifiers::COMMAND),
    );
    search(&mut app, SearchMessage::Choose(None));
    assert!(app.search.is_none());
    let listing = app.listing.as_ref().unwrap();
    assert_eq!(listing.count, 2);
    let lines: Vec<usize> = listing.hits.iter().map(|h| h.lines.len()).collect();
    assert_eq!(lines, [1, 2], "most recent first, each with its lines");
    assert_eq!(
        crate::search_view::listing_heading(listing),
        "Matching hotel: 2 notes"
    );
    // A note opened from it keeps it; Clear, or a tag, takes it away.
    let first = listing.hits[0].lines[0].2.clone();
    let path = listing.hits[0].path.clone();
    search(&mut app, SearchMessage::Go(path, first));
    assert!(app.listing.is_some());
    search(&mut app, SearchMessage::Clear);
    assert!(app.listing.is_none());
    std::fs::remove_dir_all(&dir).unwrap();
}
