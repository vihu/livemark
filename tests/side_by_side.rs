//! Side by side through iced's headless simulator (PLAN-003): the wheel
//! over the rendered pane scrolls the markdown with it, and a click in the
//! rendered pane puts the caret where it was clicked.
use iced::{Event, Point, mouse};
use livemark::widget::{Editor, Message, Mode};

const SIZE: (f32, f32) = (1000.0, 600.0);

/// Feeds one round of events and applies the messages they produce.
fn run(editor: &mut Editor, events: impl FnOnce(&mut iced_test::Simulator<'_, Message>)) {
    let messages: Vec<_> = {
        let mut ui =
            iced_test::Simulator::with_size(iced::Settings::default(), SIZE, editor.view());
        events(&mut ui);
        ui.into_messages().collect()
    };
    for message in messages {
        let _ = editor.update(message);
    }
}

fn click(ui: &mut iced_test::Simulator<'_, Message>, at: Point) {
    ui.point_at(at);
    ui.simulate([
        Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
        Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
    ]);
}

/// The line the caret is on.
fn caret_line(editor: &Editor) -> usize {
    editor.text()[..editor.selection().head]
        .matches('\n')
        .count()
}

#[test]
fn the_panes_scroll_together_and_a_click_on_the_right_places_the_caret() {
    let text = "# Heading\n\nSome text in a paragraph.\n\n".repeat(60);
    let mut editor = Editor::new(text);
    editor.set_mode(Mode::Split);
    // The wheel over the rendered pane, the right half.
    run(&mut editor, |ui| {
        ui.point_at(Point::new(750.0, 300.0));
        ui.simulate([Event::Mouse(mouse::Event::WheelScrolled {
            delta: mouse::ScrollDelta::Pixels { x: 0.0, y: -2000.0 },
        })]);
    });
    // The markdown came along: a click at its top lands far down.
    run(&mut editor, |ui| click(ui, Point::new(100.0, 20.0)));
    let left = caret_line(&editor);
    assert!(left > 40, "line {left}");
    // A click at the rendered pane's top lands near the same line.
    editor.select(0, 0);
    run(&mut editor, |ui| {
        ui.point_at(Point::new(750.0, 300.0));
        ui.simulate([Event::Mouse(mouse::Event::WheelScrolled {
            delta: mouse::ScrollDelta::Pixels { x: 0.0, y: -2000.0 },
        })]);
    });
    run(&mut editor, |ui| click(ui, Point::new(560.0, 30.0)));
    let right = caret_line(&editor);
    assert!(right > 40 && right.abs_diff(left) <= 4, "{right} vs {left}");
}
