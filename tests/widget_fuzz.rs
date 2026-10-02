//! Random sessions through the widget in iced's headless simulator: clicks
//! (one to three), drags, the wheel and the scroll bar, typing, and the
//! editing, motion, formatting, line and mode keys the surface handles.
//! Every round is drawn. Never a panic, the selection always on
//! character boundaries, and
//! undoing everything gives back the text the session started with.
//! Seeded, like `doc_fuzz.rs`; fewer seeds by default, since every round
//! builds an interface.
use common::Rng;
use iced::keyboard::{self, Key, key::Named};
use iced::{Event, Point, mouse};
use livemark::widget::{Editor, Message};

mod common;

const SEEDS: u64 = 3;
const ROUNDS: u64 = 16;
const SIZE: (f32, f32) = (500.0, 300.0);

/// Markdown in pieces, long enough to wrap and to scroll.
const PIECES: &[&str] = &[
    "==",
    " #tag",
    "[[li",
    "![a](p.png)",
    "# ",
    "## ",
    "- ",
    "1. ",
    "> ",
    "- [ ] ",
    "**",
    "*",
    "`",
    "~~",
    "[a](b)",
    "www.x.y ",
    "word ",
    "a longer stretch of words that wraps at the width ",
    "é",
    "😀",
    "\t",
    "\n",
    "\r\n",
    "\n\n",
    "```rust\nfn f() {}\n```\n",
    "| a | b |\n| - | - |\n| c | d |\n",
];

/// Front matter half the documents start with: folded into properties
/// until the caret goes in (PLAN-005).
const FRONT: &[&str] = &[
    "---\ntitle: Note\ntags: [a, b]\ncreated: 2026-10-02\n---\n",
    "---\r\ntags:\r\n  - a\r\nby: x\r\n---\r\n",
    "---\ntitle: x\n---\n# x\n",
];

const NAMED: &[Named] = &[
    Named::ArrowLeft,
    Named::ArrowRight,
    Named::ArrowUp,
    Named::ArrowDown,
    Named::Home,
    Named::End,
    Named::PageUp,
    Named::PageDown,
    Named::Enter,
    Named::Backspace,
    Named::Delete,
    Named::Tab,
    Named::Escape,
];

/// Letters pressed with Ctrl/Cmd, and with Shift some of the time.
const LETTERS: &[&str] = &[
    "a", "b", "i", "e", "k", "z", "y", "c", "x", "f", "h", "g", "t",
];

#[test]
fn random_widget_sessions_keep_the_text_whole() {
    for seed in common::seeds(SEEDS) {
        let mut rng = Rng(seed);
        let body: String = (0..20 + rng.below(60)).map(|_| *rng.pick(PIECES)).collect();
        let text = match rng.below(2) {
            0 => format!("{}{body}", rng.pick(FRONT)),
            _ => body,
        };
        let mut editor = Editor::new(text.clone());
        // Images pointing at `p.png` have a picture: lines grow under them.
        editor.set_image("p.png", &picture());
        // Half with links listed under the last line.
        if rng.below(2) == 0 {
            let link = livemark::widget::FooterLink {
                label: "Another note".into(),
                detail: "a longer line where the link sits, which wraps at the width".into(),
                destination: "b.md".into(),
            };
            editor.set_footer("Linked from", vec![link.clone(), link]);
        }
        let mut edited = false;
        for round in 0..ROUNDS {
            let events: Vec<Vec<Event>> = (0..1 + rng.below(4)).map(|_| event(&mut rng)).collect();
            let at = format!("seed {seed} round {round}");
            // Now and then the host selects, as `Editor::select` lets it.
            if rng.below(5) == 0 {
                let (anchor, head) = (rng.boundary(editor.text()), rng.boundary(editor.text()));
                editor.select(anchor, head);
            }
            // And zooms (`Editor::set_zoom`): layouts and grids at another size.
            if rng.below(8) == 0 {
                editor.set_zoom(*rng.pick(&[0.5, 1.0, 1.3, 2.0]));
            }
            run(&mut editor, rng.below(4) != 0, |ui| {
                for event in events {
                    for e in event {
                        if let Event::Mouse(mouse::Event::CursorMoved { position }) = e {
                            ui.point_at(position);
                        }
                        ui.simulate([e]);
                    }
                }
            });
            edited |= editor.text() != text;
            let selection = editor.selection();
            let text = editor.text();
            assert!(
                text.is_char_boundary(selection.anchor) && text.is_char_boundary(selection.head),
                "{at}: selection {selection:?}"
            );
        }
        assert!(edited, "seed {seed}: the events reached the editor");
        // Escape leaves the find bar or a selection, then undo all the way.
        run(&mut editor, true, |ui| {
            ui.simulate([press(
                Key::Named(Named::Escape),
                keyboard::Modifiers::empty(),
            )]);
            let undo = press(Key::Character("z".into()), keyboard::Modifiers::COMMAND);
            ui.simulate(std::iter::repeat_n(undo, 400));
        });
        assert_eq!(editor.text(), text, "seed {seed}: undone back to the start");
    }
}

/// Feeds a round of events to a fresh interface (focused first with a
/// click on the text when `focus`) and applies the messages.
fn run(
    editor: &mut Editor,
    focus: bool,
    events: impl FnOnce(&mut iced_test::Simulator<'_, Message>),
) {
    let messages: Vec<_> = {
        let mut ui =
            iced_test::Simulator::with_size(iced::Settings::default(), SIZE, editor.view());
        if focus {
            ui.point_at(Point::new(40.0, 30.0));
            ui.simulate(click());
        }
        events(&mut ui);
        // Drawn too: marks, grids and highlighting must not panic either.
        ui.snapshot(&iced::Theme::Light).expect("a frame");
        ui.into_messages().collect()
    };
    for message in messages {
        let _ = editor.update(message);
    }
}

fn click() -> [Event; 2] {
    [
        Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
        Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
    ]
}

fn moved(at: Point) -> Event {
    Event::Mouse(mouse::Event::CursorMoved { position: at })
}

fn press(key: Key, modifiers: keyboard::Modifiers) -> Event {
    Event::Keyboard(keyboard::Event::KeyPressed {
        key: key.clone(),
        modified_key: key,
        physical_key: keyboard::key::Physical::Unidentified(
            keyboard::key::NativeCode::Unidentified,
        ),
        location: keyboard::Location::Standard,
        modifiers,
        text: None,
        repeat: false,
    })
}

/// One random user action, as the events it is made of.
fn event(rng: &mut Rng) -> Vec<Event> {
    let shift = |on: bool| {
        if on {
            keyboard::Modifiers::SHIFT
        } else {
            keyboard::Modifiers::empty()
        }
    };
    match rng.below(9) {
        0 => {
            let at = point(rng);
            let clicks = 1 + rng.below(3) as usize;
            let mut events = vec![moved(at)];
            events.extend(std::iter::repeat_n(click(), clicks).flatten());
            events
        }
        1 => {
            let (from, to) = (point(rng), point(rng));
            vec![
                moved(from),
                Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                moved(to),
                Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
            ]
        }
        2 => {
            let y = rng.below(200) as f32 - 100.0;
            vec![Event::Mouse(mouse::Event::WheelScrolled {
                delta: mouse::ScrollDelta::Pixels { x: 0.0, y },
            })]
        }
        3 => {
            // The scroll bar's track, down the right edge.
            let at = Point::new(SIZE.0 - 7.0, rng.below(SIZE.1 as u64) as f32);
            let mut events = vec![moved(at)];
            events.extend(click());
            events
        }
        4 | 5 => {
            let piece = *rng.pick::<&str>(PIECES);
            piece
                .chars()
                .map(|c| {
                    Event::Keyboard(keyboard::Event::KeyPressed {
                        key: Key::Character(c.to_string().into()),
                        modified_key: Key::Character(c.to_string().into()),
                        physical_key: keyboard::key::Physical::Unidentified(
                            keyboard::key::NativeCode::Unidentified,
                        ),
                        location: keyboard::Location::Standard,
                        modifiers: keyboard::Modifiers::empty(),
                        text: Some(c.to_string().into()),
                        repeat: false,
                    })
                })
                .collect()
        }
        6 => {
            let named = *rng.pick(NAMED);
            let on = rng.below(3) == 0;
            vec![press(Key::Named(named), shift(on))]
        }
        7 => {
            let letter = *rng.pick::<&str>(LETTERS);
            let on = rng.below(4) == 0;
            vec![press(
                Key::Character(letter.into()),
                keyboard::Modifiers::COMMAND | shift(on),
            )]
        }
        _ => {
            // Line commands: Alt+Up/Down (Shift copies), Ctrl/Cmd+Enter;
            // Alt+Enter follows a link.
            let on = rng.below(2) == 0;
            let key = *rng.pick(&[Named::ArrowUp, Named::ArrowDown, Named::Enter]);
            let modifiers = match key {
                Named::Enter if on => keyboard::Modifiers::ALT,
                Named::Enter => keyboard::Modifiers::COMMAND,
                _ => keyboard::Modifiers::ALT | shift(on),
            };
            vec![press(Key::Named(key), modifiers)]
        }
    }
}

/// A random point in the editor.
fn point(rng: &mut Rng) -> Point {
    Point::new(
        rng.below(SIZE.0 as u64) as f32,
        rng.below(SIZE.1 as u64) as f32,
    )
}

/// The user's own notes, or any directory of markdown, named by
/// `LIVEMARK_EXTRA_FIXTURES` (skipped when unset): each file opens, draws,
/// takes clicks, typing and scrolling, and undoes back to itself.
#[test]
fn extra_fixtures_draw_and_edit() {
    let Ok(dir) = std::env::var("LIVEMARK_EXTRA_FIXTURES") else {
        return;
    };
    let mut files = Vec::new();
    markdown_files(std::path::Path::new(&dir), &mut files);
    for path in &files {
        let Ok(text) = std::fs::read_to_string(path) else {
            continue;
        };
        let mut rng = Rng(text.len() as u64);
        let mut editor = Editor::new(text.clone());
        for _ in 0..6 {
            let events: Vec<Vec<Event>> = (0..3).map(|_| event(&mut rng)).collect();
            run(&mut editor, true, |ui| {
                for event in events {
                    for e in event {
                        if let Event::Mouse(mouse::Event::CursorMoved { position }) = e {
                            ui.point_at(position);
                        }
                        ui.simulate([e]);
                    }
                }
            });
        }
        run(&mut editor, true, |ui| {
            ui.simulate([press(
                Key::Named(Named::Escape),
                keyboard::Modifiers::empty(),
            )]);
            let undo = press(Key::Character("z".into()), keyboard::Modifiers::COMMAND);
            ui.simulate(std::iter::repeat_n(undo, 200));
        });
        assert_eq!(editor.text(), text, "{}", path.display());
    }
    eprintln!("{} files drawn and edited", files.len());
}

fn markdown_files(dir: &std::path::Path, files: &mut Vec<std::path::PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            markdown_files(&path, files);
        } else if path.extension().is_some_and(|e| e == "md") {
            files.push(path);
        }
    }
}

/// A small PNG, 120 by 60.
fn picture() -> Vec<u8> {
    let mut bytes = Vec::new();
    image::RgbaImage::new(120, 60)
        .write_to(
            &mut std::io::Cursor::new(&mut bytes),
            image::ImageFormat::Png,
        )
        .unwrap();
    bytes
}
