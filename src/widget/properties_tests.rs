//! The front matter as properties (PLAN-005): folded while the selection is
//! outside it, its crosses and "+ tag", Ctrl/Cmd+T, completion in the
//! `tags` list, and the ways into the YAML.
use iced::Point;

use super::complete::CompleteInput;
use super::properties::Hit;
use super::{Choice, Complete, Editor, Input, Key, Message, Vertical};
use crate::edit::properties::Chip;

const NOTE: &str =
    "---\ntitle: Lisbon\ntags: [work, travel]\ncreated: 2026-10-02\n---\nPlan #ideas here.\n";

/// An editor laid out 600 wide, nothing placed yet.
fn sized(text: &str) -> Editor {
    let editor = Editor::new(text.into());
    {
        let mut lines = editor.lines.borrow_mut();
        lines.outer = iced::Size::new(600.0, 400.0);
        lines.fit();
        lines.sized = true;
    }
    editor
}

fn heights(editor: &Editor) -> Vec<f32> {
    editor.with_lines(|lines, source| {
        (0..source.doc.line_count())
            .map(|i| lines.shaped(source, i).height)
            .collect()
    })
}

fn click(editor: &mut Editor, at: Point) {
    let _ = editor.update(Message(Input::Press {
        at,
        shift: false,
        clicks: 1,
        command: false,
        other: false,
    }));
    let _ = editor.update(Message(Input::Release));
}

/// Where a control of the properties is, in the text area.
fn control(editor: &Editor, hit: &Hit) -> Point {
    editor.with_lines(|lines, source| {
        let controls = lines.block(source).controls();
        controls.iter().find(|(h, _)| h == hit).unwrap().1
    })
}

#[test]
fn front_matter_folds_into_the_properties_until_the_caret_goes_in() {
    let mut editor = sized(NOTE);
    let folded = heights(&editor);
    assert!(folded[0] > 40.0, "the title and a row of chips: {folded:?}");
    assert_eq!(&folded[1..5], [0.0; 4], "the other lines fold away");
    assert!(folded[5] > 0.0);
    // The caret in the YAML shows it, line by line.
    editor.select(8, 8);
    assert!(heights(&editor)[..5].iter().all(|&h| h > 10.0 && h < 40.0));
    // A note opening with its own heading draws no second title.
    let titled = sized("---\ntitle: Lisbon\n---\n# Lisbon\n");
    let plain = sized("---\ntitle: Lisbon\n---\nbody\n");
    assert!(heights(&titled)[0] < heights(&plain)[0]);
}

#[test]
fn a_cross_takes_its_tag_out_in_one_undo_step() {
    let mut editor = sized(NOTE);
    let end = NOTE.len();
    editor.select(end, end);
    let at = control(&editor, &Hit::Remove(Chip::Listed(1)));
    click(&mut editor, at);
    assert_eq!(
        editor.text(),
        NOTE.replace("[work, travel]", "[work]"),
        "#travel out"
    );
    assert_eq!(
        editor.selection().head,
        end - ", travel".len(),
        "the caret stays"
    );
    let at = control(&editor, &Hit::Remove(Chip::Inline("ideas".into())));
    click(&mut editor, at);
    assert!(
        editor.text().ends_with("Plan ideas here.\n"),
        "the word stays"
    );
    let _ = editor.update(Message(Input::Key(Key::Undo)));
    let _ = editor.update(Message(Input::Key(Key::Undo)));
    assert_eq!(editor.text(), NOTE);
}

#[test]
fn plus_tag_puts_the_caret_in_the_list_where_completion_leaves_out_the_hash() {
    let mut editor = sized(NOTE);
    let at = control(&editor, &Hit::Add);
    click(&mut editor, at);
    let caret = editor.selection().head;
    assert_eq!(
        &editor.text()[..caret],
        "---\ntitle: Lisbon\ntags: [work, travel, "
    );
    let completing = editor.completing().unwrap();
    assert_eq!(
        (completing.kind, completing.query.as_str()),
        (Complete::Tag, "")
    );
    let _ = editor.update(Message(Input::Commit("tr".into())));
    assert_eq!(editor.completing().unwrap().query, "tr");
    editor.set_choices(vec![Choice {
        label: "#trips".into(),
        detail: String::new(),
        insert: "#trips".into(),
    }]);
    let _ = editor.update(Message(Input::Complete(CompleteInput::Accept(None))));
    assert!(editor.text().contains("tags: [work, travel, trips]"));
}

#[test]
fn ctrl_t_makes_front_matter_when_there_is_none() {
    let mut editor = sized("Body.\n");
    let _ = editor.update(Message(Input::Key(Key::AddTag)));
    assert_eq!(editor.text(), "---\ntags: []\n---\nBody.\n");
    assert_eq!(editor.selection().head, "---\ntags: [".len());
    assert_eq!(editor.completing().map(|c| c.query), Some(String::new()));
}

#[test]
fn up_from_the_first_line_or_a_click_on_the_title_goes_into_the_yaml() {
    let mut editor = sized(NOTE);
    let body = NOTE.find("Plan").unwrap();
    editor.select(body, body);
    let _ = editor.update(Message(Input::Key(Key::Vertical(Vertical::Up, false))));
    assert_eq!(editor.selection().head, body - 1, "the closing fence's end");
    assert!(heights(&editor)[1] > 0.0, "the YAML shows");
    // A click on the title: the caret at its end.
    let mut editor = sized(NOTE);
    click(&mut editor, Point::new(20.0, 10.0));
    assert_eq!(editor.selection().head, "---\ntitle: Lisbon".len());
}
