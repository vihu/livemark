//! The search field in the toolbar, its drop-down, and the sidebar's list
//! of every match (`search.rs` finds them).
use iced::widget::text::{Ellipsis, Wrapping};
use iced::widget::{
    Space, button, column, container, lazy, mouse_area, opaque, responsive, row, rule, stack, text,
    text_input,
};
use iced::{Element, Length, Theme};

use super::search::{FIELD, Found, Hit, Listing, NOTES, SearchMessage};
use super::tag_actions::notes;
use super::{App, Message};

/// The field's width at most, and the drop-down's.
const FIELD_WIDTH: f32 = 440.0;
const PANEL_WIDTH: f32 = 580.0;

/// The command key as the platform names it.
const COMMAND: &str = if cfg!(target_os = "macos") {
    "Cmd"
} else {
    "Ctrl"
};

impl App {
    /// The field, centred in the room the toolbar leaves it, at most 440
    /// wide, with its key at its right end.
    pub(crate) fn search_field(&self) -> Element<'_, Message> {
        let field = responsive(move |size| -> Element<'_, Message> {
            let query = self.search.as_ref().map_or("", |s| s.query.as_str());
            let placeholder = if self.vault.is_some() {
                "Search notes, text and #tags"
            } else {
                "Open a recent file by name"
            };
            let hint = if self.search.is_some() {
                "Esc".to_owned()
            } else {
                format!("{COMMAND}+P")
            };
            let input = text_input(placeholder, query)
                .id(FIELD)
                .on_input(|query| Message::Search(SearchMessage::Query(query)))
                .on_submit(Message::Search(SearchMessage::Choose(None)))
                .size(13)
                .padding(iced::Padding {
                    top: 7.0,
                    right: 60.0,
                    bottom: 7.0,
                    left: 12.0,
                })
                .style(field_style);
            let hint = container(text(hint).size(11).style(text::secondary))
                .width(Length::Fill)
                .height(Length::Fill)
                .align_x(iced::alignment::Horizontal::Right)
                .align_y(iced::Center)
                .padding([0, 12]);
            container(stack![input, hint].width(size.width.min(FIELD_WIDTH)))
                .center_x(Length::Fill)
                .into()
        });
        container(field).height(34).into()
    }

    /// The drop-down under the field while it is open; a press outside it
    /// closes it.
    pub(crate) fn search_panel(&self) -> Option<Element<'_, Message>> {
        let search = self.search.as_ref()?;
        let mut list = column![].spacing(1);
        let mut group = "";
        for (i, found) in search.found.iter().enumerate() {
            let name = match found {
                Found::Note(..) if self.vault.is_none() => "Recent files",
                Found::Note(..) if search.query.trim().is_empty() => "Recent notes",
                Found::Note(..) => "Notes",
                Found::Line { .. } => "In text",
                Found::Tag(..) => "Tags",
            };
            if name != group {
                group = name;
                list = list.push(
                    container(text(name).size(11).style(text::secondary)).padding(iced::Padding {
                        top: 6.0,
                        right: 10.0,
                        bottom: 2.0,
                        left: 10.0,
                    }),
                );
            }
            let (title, under, side) = match found {
                Found::Note(title, meta, _) => (title.clone(), meta.clone(), String::new()),
                Found::Line {
                    title,
                    snippet,
                    number,
                    ..
                } => (title.clone(), snippet.clone(), format!("line {number}")),
                Found::Tag(tag, count) => (format!("#{tag}"), String::new(), notes(*count)),
            };
            let on = i == search.selected;
            let entry = column![
                text(title)
                    .size(14)
                    .wrapping(Wrapping::None)
                    .ellipsis(Ellipsis::End),
            ]
            .push((!under.is_empty()).then(|| {
                text(under)
                    .size(12)
                    .wrapping(Wrapping::None)
                    .ellipsis(Ellipsis::End)
                    .style(move |theme: &Theme| text::Style {
                        color: Some(row_text(theme, on).scale_alpha(0.65)),
                    })
            }))
            .width(Length::Fill);
            list = list.push(
                button(
                    row![
                        entry,
                        text(side).size(12).style(move |theme: &Theme| text::Style {
                            color: Some(row_text(theme, on).scale_alpha(0.65)),
                        })
                    ]
                    .spacing(10)
                    .align_y(iced::Center),
                )
                .width(Length::Fill)
                .padding([5, 10])
                .style(move |theme: &Theme, status| {
                    let palette = theme.palette();
                    let background = match status {
                        _ if on => Some(palette.primary.weak.color),
                        button::Status::Hovered => Some(palette.background.weak.color),
                        _ => None,
                    };
                    button::Style {
                        background: background.map(iced::Background::Color),
                        text_color: row_text(theme, on),
                        border: iced::Border::default().rounded(6),
                        ..button::Style::default()
                    }
                })
                .on_press(Message::Search(SearchMessage::Choose(Some(i)))),
            );
        }
        if search.found.is_empty() {
            let said = if search.query.trim().is_empty() {
                "Type to search"
            } else {
                "Nothing matches"
            };
            list =
                list.push(container(text(said).size(13).style(text::secondary)).padding([8, 10]));
        }
        // What the keys do now.
        let enter = search.found.get(search.selected).map(|found| match found {
            Found::Note(title, ..) => format!("Enter opens {title}"),
            Found::Line { title, number, .. } => format!("Enter opens {title} at line {number}"),
            Found::Tag(tag, _) => format!("Enter shows the notes tagged #{tag}"),
        });
        let all = (self.vault.is_some() && search.count > 0).then(|| {
            format!(
                "{COMMAND}+Enter lists {} in the sidebar",
                if search.count == 1 {
                    "the 1 match".to_owned()
                } else {
                    format!("all {} matches", search.count)
                }
            )
        });
        let footer = row![]
            .push(enter.map(|said| text(said).size(11).style(text::secondary)))
            .push(text("Up and Down choose").size(11).style(text::secondary))
            .push(all.map(|said| text(said).size(11).style(text::secondary)))
            .spacing(14)
            .wrap();
        let panel = container(column![
            list,
            rule::horizontal(1),
            container(footer).padding([6, 10])
        ])
        .width(PANEL_WIDTH)
        .padding(6)
        .style(container::bordered_box);
        Some(
            stack![
                mouse_area(Space::new().width(Length::Fill).height(Length::Fill))
                    .on_press(Message::Search(SearchMessage::Close)),
                container(opaque(panel))
                    .center_x(Length::Fill)
                    .padding([4, 12]),
            ]
            .into(),
        )
    }

    /// The sidebar's list of every match, in place of the notes.
    pub(crate) fn listing_view<'a>(&'a self, listing: &'a Listing) -> Element<'a, Message> {
        let current = self.path.clone();
        lazy((listing.generation, current), move |(_, current)| {
            results(&listing.hits, current.as_deref())
        })
        .into()
    }
}

/// What heads the sidebar's list of matches.
pub(crate) fn listing_heading(listing: &Listing) -> String {
    let count = match listing.count {
        0 => "nothing".to_owned(),
        n if n > NOTES => format!("{n} notes, the {NOTES} most recent shown"),
        n => notes(n),
    };
    format!("Matching {}: {count}", listing.query.trim())
}

/// The field: shaded until it has focus, then outlined in the accent.
fn field_style(theme: &Theme, status: text_input::Status) -> text_input::Style {
    let palette = theme.palette();
    let mut style = text_input::default(theme, status);
    let focused = matches!(status, text_input::Status::Focused { .. });
    style.background = if focused {
        palette.background.base.color
    } else {
        palette.background.weak.color
    }
    .into();
    style.border = iced::Border {
        color: if focused {
            palette.primary.base.color
        } else {
            iced::Color::TRANSPARENT
        },
        width: 1.0,
        radius: 8.0.into(),
    };
    style
}

fn row_text(theme: &Theme, on: bool) -> iced::Color {
    let palette = theme.palette();
    if on {
        palette.primary.weak.text
    } else {
        palette.background.base.text
    }
}

/// The matches: each note's title, then its lines, every one a link to
/// the match.
fn results(hits: &[Hit], current: Option<&std::path::Path>) -> Element<'static, Message> {
    let mut list = column![].spacing(2);
    for hit in hits {
        let first = hit.lines.first().map_or(0..0, |line| line.2.clone());
        let on = current == Some(hit.path.as_path());
        list = list.push(
            button(
                text(hit.title.clone())
                    .size(14)
                    .wrapping(Wrapping::None)
                    .ellipsis(Ellipsis::End),
            )
            .width(Length::Fill)
            .padding([4, 8])
            .style(move |theme: &Theme, status| super::sidebar::choice(theme, status, on))
            .on_press(Message::Search(SearchMessage::Go(hit.path.clone(), first))),
        );
        for (number, snippet, range) in &hit.lines {
            list = list.push(
                button(
                    row![
                        text(number.to_string())
                            .size(11)
                            .width(28)
                            .style(text::secondary),
                        text(snippet.clone())
                            .size(12)
                            .wrapping(Wrapping::None)
                            .ellipsis(Ellipsis::End),
                    ]
                    .spacing(4),
                )
                .width(Length::Fill)
                .padding([2, 8])
                .style(|theme: &Theme, status| super::sidebar::choice(theme, status, false))
                .on_press(Message::Search(SearchMessage::Go(
                    hit.path.clone(),
                    range.clone(),
                ))),
            );
        }
        list = list.push(Space::new().height(4));
    }
    container(list).into()
}
