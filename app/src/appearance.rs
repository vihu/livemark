//! Appearance (PLAN-006), in the note's place as the tag manager is: the
//! theme, every one drawn as a little window in its own colours, System
//! following the system between a light and a dark one picked; the whole
//! interface's scale (Ctrl+=, Ctrl+- and Ctrl+0) and the note's text size
//! on top of it.
use iced::theme::Mode;
use iced::widget::{Space, button, column, container, pick_list, row, scrollable, text};
use iced::{Color, Element, Length, Task, Theme};

use super::icons::{Icon, Tone, icon};
use super::settings::Theme as Named;
use super::shell::COMMAND;
use super::{App, Message};

#[derive(Debug, Clone)]
pub enum AppearanceMessage {
    Open,
    Close,
    Theme(Named),
    /// What System follows by night (`dark`) or by day.
    Pick {
        dark: bool,
        theme: Named,
    },
    /// The interface a tenth bigger (1), smaller (-1), or back to 100% (0).
    Scale(i8),
    /// The system went light or dark.
    System(Mode),
}

/// The iced theme for a named one; `None` for System.
pub fn iced(theme: Named) -> Option<Theme> {
    Some(match theme {
        Named::System => return None,
        Named::Light => Theme::Light,
        Named::Dark => Theme::Dark,
        Named::KanagawaWave => Theme::KanagawaWave,
        Named::KanagawaLotus => Theme::KanagawaLotus,
        Named::SolarizedLight => Theme::SolarizedLight,
        Named::SolarizedDark => Theme::SolarizedDark,
        Named::GruvboxLight => Theme::GruvboxLight,
        Named::GruvboxDark => Theme::GruvboxDark,
        Named::CatppuccinMocha => Theme::CatppuccinMocha,
        Named::CatppuccinFrappe => Theme::CatppuccinFrappe,
    })
}

impl App {
    /// The theme drawn now: a `--dark` or `--light` flag's, else the
    /// settings', System following the system once it has said.
    pub(crate) fn current_theme(&self) -> Option<Theme> {
        if let Some(forced) = &self.theme {
            return Some(forced.clone());
        }
        match self.settings.theme {
            Named::System => match self.system_mode {
                Mode::Dark => iced(self.settings.dark),
                Mode::Light => iced(self.settings.light),
                Mode::None => None,
            },
            named => iced(named),
        }
    }

    pub(crate) fn appearance_update(&mut self, message: AppearanceMessage) -> Task<Message> {
        match message {
            AppearanceMessage::Open => {
                self.menu = false;
                self.manager = None;
                self.appearance = true;
            }
            AppearanceMessage::Close => {
                self.appearance = false;
                return livemark::widget::Editor::focus();
            }
            AppearanceMessage::Theme(theme) => {
                // Picked here, it wins over a flag given at the start.
                self.theme = None;
                self.settings.theme = theme;
                self.remember();
            }
            AppearanceMessage::Pick { dark, theme } => {
                if dark {
                    self.settings.dark = theme;
                } else {
                    self.settings.light = theme;
                }
                self.theme = None;
                self.settings.theme = Named::System;
                self.remember();
            }
            AppearanceMessage::Scale(step) => {
                let tenths = (self.settings.scale * 10.0).round() + f32::from(step);
                self.settings.scale = if step == 0 {
                    1.0
                } else {
                    (tenths / 10.0).clamp(0.5, 2.0)
                };
                self.remember();
            }
            AppearanceMessage::System(mode) => self.system_mode = mode,
        }
        Task::none()
    }

    /// The panel: the themes, then the sizes.
    pub(crate) fn appearance_view(&self) -> Element<'_, Message> {
        let send = |message| Message::Appearance(message);
        let head = row![
            column![
                text("Appearance").size(26).font(iced::Font {
                    weight: iced::font::Weight::Bold,
                    ..iced::Font::new(livemark::fonts::PROSE)
                }),
                text("Kept with your settings.")
                    .size(13)
                    .style(text::secondary),
            ]
            .spacing(4)
            .width(Length::Fill),
            button(text("Back to the note").size(13))
                .padding([6, 12])
                .style(button::secondary)
                .on_press(send(AppearanceMessage::Close)),
        ]
        .align_y(iced::Bottom);
        // Shown as picked: a flag given at the start is not a pick.
        let picked = self.settings.theme;
        let mut cards: Vec<Element<'_, Message>> = vec![self.system_card(picked == Named::System)];
        cards.extend(
            Named::NAMED
                .iter()
                .map(|&theme| theme_card(theme, theme == picked)),
        );
        let mut grid = column![].spacing(12);
        let mut cards = cards.into_iter().peekable();
        while cards.peek().is_some() {
            let mut line = row![].spacing(12);
            for _ in 0..4 {
                line = line.push(
                    container(cards.next().unwrap_or_else(|| Space::new().into()))
                        .width(Length::Fill),
                );
            }
            grid = grid.push(line);
        }
        let sizes = row![
            size_box(
                "Interface",
                format!(
                    "Everything: the sidebar, menus, bars and the text. {COMMAND}+= and {COMMAND}+-, {COMMAND}+0 back."
                ),
                self.settings.scale,
                send(AppearanceMessage::Scale(-1)),
                send(AppearanceMessage::Scale(1)),
            ),
            size_box(
                "Text",
                "The note's text, on top of the interface's size.".into(),
                self.editor.zoom(),
                Message::Zoom(-1),
                Message::Zoom(1),
            ),
        ]
        .spacing(12);
        let heading = |said: &'static str| text(said).size(13).style(text::secondary);
        scrollable(
            column![
                head,
                Space::new().height(8),
                heading("Theme"),
                grid,
                Space::new().height(8),
                heading("Size"),
                sizes,
            ]
            .spacing(12)
            .padding(iced::Padding {
                top: 26.0,
                right: 40.0,
                bottom: 40.0,
                left: 48.0,
            }),
        )
        .height(Length::Fill)
        .into()
    }

    /// System's card: the day theme and the night theme side by side, and
    /// what each is.
    fn system_card(&self, on: bool) -> Element<'_, Message> {
        let (day, night) = (self.settings.light, self.settings.dark);
        let half = |theme: Named, said: &'static str| {
            let palette = *iced(theme).unwrap_or(Theme::Light).palette();
            let (background, ink) = (palette.background.base.color, palette.background.base.text);
            container(text(said).size(12).color(ink))
                .width(Length::Fill)
                .height(84)
                .align_x(iced::alignment::Horizontal::Center)
                .align_y(iced::Center)
                .style(move |_: &Theme| container::Style {
                    background: Some(background.into()),
                    ..container::Style::default()
                })
        };
        let lights: Vec<Named> = Named::NAMED.into_iter().filter(|t| !t.is_dark()).collect();
        let darks: Vec<Named> = Named::NAMED.into_iter().filter(|t| t.is_dark()).collect();
        let picks = row![
            column![
                text("By day").size(11).style(text::secondary),
                pick_list(Some(day), lights, |theme: &Named| theme.name().to_owned())
                    .on_select(|theme| {
                        Message::Appearance(AppearanceMessage::Pick { dark: false, theme })
                    })
                    .text_size(12)
                    .padding([3, 6]),
            ]
            .spacing(2)
            .width(Length::Fill),
            column![
                text("By night").size(11).style(text::secondary),
                pick_list(Some(night), darks, |theme: &Named| theme.name().to_owned())
                    .on_select(|theme| {
                        Message::Appearance(AppearanceMessage::Pick { dark: true, theme })
                    })
                    .text_size(12)
                    .padding([3, 6]),
            ]
            .spacing(2)
            .width(Length::Fill),
        ]
        .spacing(8);
        let preview = container(row![half(day, "Day"), half(night, "Night")])
            .clip(true)
            .style(|theme: &Theme| container::Style {
                border: iced::Border {
                    color: theme.palette().background.strong.color,
                    width: 1.0,
                    radius: 8.0.into(),
                },
                ..container::Style::default()
            });
        button(
            column![
                preview,
                row![text("System").size(13).width(Length::Fill)].push(on.then(|| icon(
                    Icon::Check,
                    16.0,
                    Tone::Quiet
                ))),
                picks,
            ]
            .spacing(8),
        )
        .padding(8)
        .width(Length::Fill)
        .style(move |theme: &Theme, _| card_style(theme.palette(), on))
        .on_press(Message::Appearance(AppearanceMessage::Theme(Named::System)))
        .into()
    }
}

/// A theme's card: a little window in its colours, and its name.
fn theme_card<'a>(named: Named, on: bool) -> Element<'a, Message> {
    let palette = *iced(named).unwrap_or(Theme::Light).palette();
    let page = palette.background.base.color;
    let side = palette.background.weakest.color;
    let ink = palette.background.base.text;
    let muted = ink.scale_alpha(0.45);
    let accent = palette.primary.base.color;
    let pill = accent.scale_alpha(0.28);
    let bar = |width: f32, height: f32, color: Color| {
        container(Space::new().width(width).height(height)).style(move |_: &Theme| {
            container::Style {
                background: Some(color.into()),
                border: iced::Border::default().rounded(height / 2.0),
                ..container::Style::default()
            }
        })
    };
    let preview = container(row![
        container(
            column![
                bar(30.0, 5.0, muted),
                bar(22.0, 5.0, muted),
                bar(26.0, 5.0, accent),
                bar(20.0, 5.0, muted)
            ]
            .spacing(5)
        )
        .padding([10, 7])
        .width(56)
        .height(Length::Fill)
        .style(move |_: &Theme| container::Style {
            background: Some(side.into()),
            ..container::Style::default()
        }),
        column![
            bar(64.0, 7.0, ink),
            row![bar(22.0, 9.0, pill), bar(22.0, 9.0, pill)].spacing(4),
            bar(92.0, 5.0, muted),
            bar(74.0, 5.0, muted),
        ]
        .spacing(7)
        .padding([10, 10]),
    ])
    .width(Length::Fill)
    .height(84)
    .clip(true)
    .style(move |_: &Theme| container::Style {
        background: Some(page.into()),
        border: iced::Border {
            color: ink.scale_alpha(0.12),
            width: 1.0,
            radius: 8.0.into(),
        },
        ..container::Style::default()
    });
    button(
        column![
            preview,
            row![text(named.name()).size(13).color(ink).width(Length::Fill)]
                .push(on.then(|| text("\u{2713}").size(14).color(accent))),
        ]
        .spacing(8),
    )
    .padding(8)
    .width(Length::Fill)
    .style(move |_: &Theme, _| card_style(&palette, on))
    .on_press(Message::Appearance(AppearanceMessage::Theme(named)))
    .into()
}

/// A card on its own theme's page, ringed in the accent when picked.
fn card_style(palette: &iced::theme::palette::Palette, on: bool) -> button::Style {
    button::Style {
        background: Some(palette.background.base.color.into()),
        text_color: palette.background.base.text,
        border: iced::Border {
            color: if on {
                palette.primary.base.color
            } else {
                palette.background.strong.color
            },
            width: if on { 2.0 } else { 1.0 },
            radius: 12.0.into(),
        },
        ..button::Style::default()
    }
}

/// A size with what it covers, and its steps.
fn size_box<'a>(
    name: &'static str,
    covers: String,
    value: f32,
    smaller: Message,
    bigger: Message,
) -> Element<'a, Message> {
    let step = |glyph: Icon, message: Message| {
        button(icon(glyph, 12.0, Tone::Quiet))
            .padding([6, 8])
            .style(button::secondary)
            .on_press(message)
    };
    container(
        row![
            column![
                text(name).size(14),
                text(covers).size(12).style(text::secondary)
            ]
            .spacing(2)
            .width(Length::Fill),
            step(Icon::Minus, smaller),
            text(format!("{:.0}%", value * 100.0))
                .size(13)
                .width(52)
                .center(),
            step(Icon::Plus, bigger),
        ]
        .spacing(8)
        .align_y(iced::Center),
    )
    .padding([12, 14])
    .width(Length::Fill)
    .style(|theme: &Theme| container::Style {
        border: iced::Border {
            color: theme.palette().background.strong.color,
            width: 1.0,
            radius: 12.0.into(),
        },
        ..container::Style::default()
    })
    .into()
}
