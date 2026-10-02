//! The search field in the toolbar, its drop-down, and the sidebar's list
//! of every match (`search.rs` finds them).
use iced::widget::text::{Ellipsis, Wrapping};
use iced::widget::{
    Space, button, column, container, lazy, mouse_area, opaque, rich_text, row, rule, span, stack,
    text, text_input,
};
use iced::{Element, Length, Theme};

use super::icons::{Icon, Tone, icon};
use super::matching::occurrences;
use super::search::{FIELD, Found, Hit, Listing, NOTES, SearchMessage, split};
use super::shell::COMMAND;
use super::tag_actions::notes;
use super::{App, Message};

/// The drop-down's width.
const PANEL_WIDTH: f32 = 580.0;

impl App {
    /// The field, `width` wide, with its key at its right end.
    pub(crate) fn search_field(&self, width: f32) -> Element<'_, Message> {
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
                left: 34.0,
            })
            .style(field_style);
        let glass = container(icon(Icon::Search, 16.0, Tone::Quiet))
            .height(Length::Fill)
            .align_y(iced::Center)
            .padding([0, 11]);
        let hint = container(text(hint).size(11).style(text::secondary))
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(iced::alignment::Horizontal::Right)
            .align_y(iced::Center)
            .padding([0, 12]);
        container(stack![input, glass, hint]).width(width).into()
    }

    /// The drop-down under the field while it is open; a press outside it
    /// closes it.
    pub(crate) fn search_panel(&self) -> Option<Element<'_, Message>> {
        let search = self.search.as_ref()?;
        // What to mark: the words in lines, `#tag`s in tag lines.
        let (words, tags) = split(&search.query);
        let hashes: Vec<String> = tags
            .iter()
            .chain(&words)
            .map(|tag| format!("#{tag}"))
            .collect();
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
            // Each line with what matched marked.
            let (title, title_marks, under, under_marks, side) = match found {
                Found::Note(title, meta, _, marks) => (
                    title.clone(),
                    marks.clone(),
                    meta.clone(),
                    occurrences(meta, &hashes),
                    String::new(),
                ),
                Found::Line {
                    title,
                    snippet,
                    number,
                    ..
                } => (
                    title.clone(),
                    Vec::new(),
                    snippet.clone(),
                    occurrences(snippet, &words),
                    format!("line {number}"),
                ),
                Found::Tag(tag, count) => {
                    let name = format!("#{tag}");
                    let marks = occurrences(&name, &hashes);
                    (name, marks, String::new(), Vec::new(), notes(*count))
                }
            };
            let on = i == search.selected;
            let entry = column![marked(title, &title_marks, 14.0, false, on)]
                .push((!under.is_empty()).then(|| marked(under, &under_marks, 12.0, true, on)))
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
        // Nothing typed yet: how the search matches, instead.
        let footer = if search.query.trim().is_empty() {
            row![
                text("Titles: words in any order, a typo forgiven. Text: every word as typed, any case. #tag narrows to a tag.")
                    .size(11)
                    .style(text::secondary)
            ]
        } else {
            row![]
                .push(enter.map(|said| text(said).size(11).style(text::secondary)))
                .push(text("Up and Down choose").size(11).style(text::secondary))
                .push(all.map(|said| text(said).size(11).style(text::secondary)))
                .spacing(14)
        }
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
            results(&listing.hits, current.as_deref(), &split(&listing.query).0)
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
fn results(
    hits: &[Hit],
    current: Option<&std::path::Path>,
    words: &[String],
) -> Element<'static, Message> {
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
                        marked(
                            snippet.clone(),
                            &occurrences(snippet, words),
                            12.0,
                            false,
                            false
                        ),
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

/// The marker pen behind what matched.
const MARK: iced::Color = iced::Color::from_rgba8(255, 214, 0, 0.38);

/// `said` on one line, cut short with an ellipsis, its `marks` (byte ranges)
/// on a marker-pen band; quieter when `quiet`, in a chosen row's colours
/// when `on`.
fn marked<'a>(
    said: String,
    marks: &[std::ops::Range<usize>],
    size: f32,
    quiet: bool,
    on: bool,
) -> Element<'a, Message> {
    let mut spans: Vec<text::Span<'a, ()>> = Vec::new();
    let mut at = 0;
    for mark in marks {
        let (start, end) = (mark.start.max(at), mark.end.min(said.len()));
        if start >= end || !said.is_char_boundary(start) || !said.is_char_boundary(end) {
            continue;
        }
        if start > at {
            spans.push(span(said[at..start].to_owned()));
        }
        spans.push(
            span(said[start..end].to_owned())
                .background(MARK)
                .border(iced::Border::default().rounded(2)),
        );
        at = end;
    }
    if at < said.len() {
        spans.push(span(said[at..].to_owned()));
    }
    rich_text(spans)
        .size(size)
        .wrapping(Wrapping::None)
        .ellipsis(Ellipsis::End)
        .style(move |theme: &Theme| {
            let color = row_text(theme, on);
            text::Style {
                color: Some(if quiet {
                    color.scale_alpha(0.65)
                } else {
                    color
                }),
            }
        })
        .into()
}
