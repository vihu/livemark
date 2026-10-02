//! Frame times for the milestone exit checks (PLAN-001): typing in the
//! middle of a large document, scrolling it, opening a 1 MB file, and
//! typing in a long code block,
//! through iced's own frame loop (build the interface, feed the event,
//! apply the messages, build again, draw) with a headless renderer.
//!
//! ```text
//! cargo run --release --example bench -- [wgpu|tiny-skia]
//! ```
//!
//! Per frame it times `update` (building the interface, the widget turning
//! the event into messages, and the editor applying them: edit, re-parse,
//! restyle, scroll), `draw` (building again, projecting and shaping the
//! lines that changed, drawing) and, on tiny-skia only, `present` (software
//! rasterization of the whole frame). On wgpu the GPU part is left out, as
//! in roughdraft's bench.
use std::time::{Duration, Instant};

use iced::advanced::renderer::Headless;
use iced::keyboard::{self, Key};
use iced::{Color, Event, Point, Size, Theme, mouse};
use iced_runtime::user_interface::{Cache, UserInterface};
use livemark::widget::{Editor, Message};

const VIEWPORT: Size = Size::new(1280.0, 800.0);
const FRAMES: usize = 120;

fn main() {
    let backend = std::env::args().nth(1).unwrap_or_else(|| "wgpu".into());
    let coverage = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/docs/coverage.md"
    ))
    .unwrap();
    let per = coverage.lines().count();

    let start = Instant::now();
    let renderer = iced::Renderer::new(
        iced::advanced::renderer::Settings::default(),
        Some(&backend),
    );
    let renderer = iced::futures::executor::block_on(renderer).expect("a headless renderer");
    let mut bench = Bench {
        renderer,
        cache: Some(Cache::default()),
        present: backend != "wgpu",
    };
    println!(
        "{}: renderer and font system ready in {:.0} ms",
        bench.renderer.name(),
        start.elapsed().as_secs_f64() * 1e3
    );

    // Opening: a document of about 1 MB to its first frame.
    let big = coverage.repeat((1 << 20) / coverage.len() + 1);
    let start = Instant::now();
    let mut editor = Editor::new(big.clone());
    let made = start.elapsed();
    bench.frame(&mut editor, &[]);
    println!(
        "open {} KB ({} lines): editor {:.1} ms, first frame {:.1} ms total",
        big.len() / 1024,
        big.lines().count(),
        made.as_secs_f64() * 1e3,
        start.elapsed().as_secs_f64() * 1e3
    );

    for lines in [1_000usize, 5_000, 20_000] {
        let text = coverage.repeat(lines.div_ceil(per));
        let mut editor = Editor::new(text);
        // Focus the widget with a click, then put the caret in the middle
        // of a paragraph line, as a host would.
        let press = Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left));
        let release = Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left));
        bench.frame(&mut editor, &[press, release]);
        let middle = editor.text().len() / 2;
        let middle = middle + editor.text()[middle..].find("Plain paragraph").unwrap() + 6;
        editor.select(middle, middle);
        bench.frame(&mut editor, &[]);

        let before = editor.text().len();
        let typing: Vec<Timing> = (0..FRAMES)
            .map(|i| {
                let c = "typing in the middle "
                    .chars()
                    .nth(i % 21)
                    .unwrap()
                    .to_string();
                bench.frame(&mut editor, &[key(&c)])
            })
            .collect();
        assert_eq!(editor.text().len(), before + FRAMES, "every key typed");
        report(&format!("{lines} lines, a keystroke"), &typing);

        let scrolling: Vec<Timing> = (0..FRAMES)
            .map(|_| {
                let wheel = Event::Mouse(mouse::Event::WheelScrolled {
                    delta: mouse::ScrollDelta::Pixels { x: 0.0, y: -40.0 },
                });
                bench.frame(&mut editor, &[wheel])
            })
            .collect();
        report(
            &format!("{lines} lines, a scroll frame (40 px)"),
            &scrolling,
        );
    }

    // Typing in a 400-line Rust block, at its top and at line 200 (scrolled
    // there): highlighting parses from the snapshot before the edited line
    // down to the last line drawn, not the whole block.
    let code: String = (0..400)
        .map(|i| format!("    let x{i} = \"s{i}\"; // n{i}\n"))
        .collect();
    for line in [0usize, 200] {
        let mut editor = Editor::new(format!("```rust\nfn main() {{\n{code}}}\n```\n"));
        let press = Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left));
        let release = Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left));
        bench.frame(&mut editor, &[press, release]);
        let at = editor.text().find(&format!("let x{line} ")).unwrap() + 5;
        editor.select(at, at);
        bench.frame(&mut editor, &[]);
        let typing: Vec<Timing> = (0..FRAMES)
            .map(|i| bench.frame(&mut editor, &[key(if i % 2 == 0 { "a" } else { "b" })]))
            .collect();
        report(
            &format!("a 400-line code block, a keystroke at line {line}"),
            &typing,
        );
    }
}

/// A key press typing `c`.
fn key(c: &str) -> Event {
    Event::Keyboard(keyboard::Event::KeyPressed {
        key: Key::Character(c.into()),
        modified_key: Key::Character(c.into()),
        physical_key: keyboard::key::Physical::Unidentified(
            keyboard::key::NativeCode::Unidentified,
        ),
        location: keyboard::Location::Standard,
        modifiers: keyboard::Modifiers::default(),
        text: Some(c.into()),
        repeat: false,
    })
}

/// The renderer and the interface's cache, kept across frames like a
/// window's.
struct Bench {
    renderer: iced::Renderer,
    cache: Option<Cache>,
    present: bool,
}

/// Time spent in each phase of one frame.
#[derive(Clone, Copy, Default)]
struct Timing {
    update: Duration,
    draw: Duration,
    present: Duration,
}

impl Bench {
    /// Feeds `events` to the editor's interface, applies the messages,
    /// then draws a frame.
    fn frame(&mut self, editor: &mut Editor, events: &[Event]) -> Timing {
        let cursor = mouse::Cursor::Available(Point::new(200.0, 200.0));
        let start = Instant::now();
        let mut messages = iced::advanced::shell::Bus::<Message>::new();
        let mut ui = UserInterface::build(
            editor.view(),
            VIEWPORT,
            self.cache.take().unwrap(),
            &mut self.renderer,
        );
        let _ = ui.update(
            &iced::window::Headless,
            &iced::advanced::shell::Waker::noop(),
            events,
            cursor,
            &mut self.renderer,
            &mut messages,
        );
        self.cache = Some(ui.into_cache());
        for message in messages {
            let _ = editor.update(message);
        }
        let update = start.elapsed();

        let start = Instant::now();
        let mut ui = UserInterface::build(
            editor.view(),
            VIEWPORT,
            self.cache.take().unwrap(),
            &mut self.renderer,
        );
        let redraw = Event::Window(iced::window::Event::RedrawRequested(Instant::now()));
        let _ = ui.update(
            &iced::window::Headless,
            &iced::advanced::shell::Waker::noop(),
            &[redraw],
            cursor,
            &mut self.renderer,
            &mut iced::advanced::shell::Bus::new(),
        );
        let theme = Theme::Light;
        ui.draw(
            &mut self.renderer,
            &theme,
            &iced::advanced::renderer::Style {
                text_color: Color::BLACK,
            },
            cursor,
        );
        self.cache = Some(ui.into_cache());
        let draw = start.elapsed();

        let start = Instant::now();
        if self.present {
            let size = Size::new(VIEWPORT.width as u32, VIEWPORT.height as u32);
            let _pixels = self.renderer.screenshot(size, 1.0, Color::WHITE);
        }
        let present = start.elapsed();
        Timing {
            update,
            draw,
            present,
        }
    }
}

fn report(name: &str, frames: &[Timing]) {
    let stats = |pick: fn(&Timing) -> Duration| {
        let mut ms: Vec<f64> = frames.iter().map(|t| pick(t).as_secs_f64() * 1e3).collect();
        ms.sort_by(f64::total_cmp);
        let at = |q: f64| ms[((ms.len() - 1) as f64 * q).round() as usize];
        format!("median {:.2} ms, p95 {:.2} ms", at(0.5), at(0.95))
    };
    println!("{name} ({} frames):", frames.len());
    println!("  update  {}", stats(|t| t.update));
    println!("  draw    {}", stats(|t| t.draw));
    println!("  present {}", stats(|t| t.present));
    println!("  total   {}", stats(|t| t.update + t.draw + t.present));
}
