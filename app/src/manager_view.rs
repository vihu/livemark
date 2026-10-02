//! The tag manager's view: the heading, the look-alike suggestion, the
//! table and the bar for the selected tags (`manager.rs` keeps the rest).
use iced::widget::text::{Ellipsis, Wrapping};
use iced::widget::{
    Space, button, checkbox, column, container, row, rule, scrollable, stack, text, text_input,
};
use iced::{Element, Length, Theme};

use super::manager::{ANOTHER, Column, Manager, ManagerMessage, plural, sorted};
use super::tag_actions::{TagAction, TagMessage, card, listed, notes};
use super::{App, Message};

impl App {
    pub(crate) fn manager_view<'a>(&'a self, manager: &'a Manager) -> Element<'a, Message> {
        let Some(vault) = &self.vault else {
            return Space::new().into();
        };
        let tags = vault.tags();
        let tagged = vault.notes.iter().filter(|n| !n.tags.is_empty()).count();
        let summary = format!(
            "{} across {}; {}",
            plural(tags.len(), "tag"),
            notes(tagged),
            match vault.notes.len() - tagged {
                0 => "every note has one".to_owned(),
                1 => "1 note has none".to_owned(),
                n => format!("{n} notes have none"),
            }
        );
        let head = row![
            column![
                text("Tags").size(26).font(iced::Font {
                    weight: iced::font::Weight::Bold,
                    ..iced::Font::new(livemark::fonts::PROSE)
                }),
                text(summary).size(13).style(text::secondary),
            ]
            .spacing(4)
            .width(Length::Fill),
            button(text("Back to the note").size(13))
                .padding([6, 12])
                .style(button::secondary)
                .on_press(Message::Manager(ManagerMessage::Close)),
        ]
        .align_y(iced::Bottom);
        let banner = self.suggestion().map(|(from, into)| {
            let count = |tag: &str| tags.iter().find(|(t, _)| t == tag).map_or(0, |(_, n)| *n);
            container(
                row![
                    text(format!(
                        "#{from} looks like #{into}: {} against {}. Merge them?",
                        notes(count(&from)),
                        count(&into)
                    ))
                    .size(13)
                    .width(Length::Fill),
                    button(text(format!("Merge into #{into}")).size(13))
                        .padding([5, 12])
                        .style(button::primary)
                        .on_press(Message::Manager(ManagerMessage::Accept(
                            from.clone(),
                            into.clone()
                        ))),
                    button(text("Keep both").size(13))
                        .padding([5, 8])
                        .style(button::text)
                        .on_press(Message::Manager(ManagerMessage::KeepBoth(from, into))),
                ]
                .spacing(12)
                .align_y(iced::Center),
            )
            .padding([10, 14])
            .style(|theme: &Theme| {
                let warning = theme.palette().warning.base.color;
                container::Style {
                    background: Some(warning.scale_alpha(0.14).into()),
                    border: iced::Border {
                        color: warning.scale_alpha(0.5),
                        width: 1.0,
                        radius: 10.0.into(),
                    },
                    ..container::Style::default()
                }
            })
        });
        let heading = |label: &'static str, column: Column, width: Length, right: bool| {
            let arrow = match (manager.sort == column, manager.descending) {
                (false, _) => "",
                (true, true) => " ↓",
                (true, false) => " ↑",
            };
            let sorted = manager.sort == column;
            button(
                text(format!("{label}{arrow}"))
                    .size(12)
                    .font(iced::Font {
                        weight: iced::font::Weight::Semibold,
                        ..iced::Font::new(livemark::fonts::PROSE)
                    })
                    .width(Length::Fill)
                    .align_x(if right {
                        iced::alignment::Horizontal::Right
                    } else {
                        iced::alignment::Horizontal::Left
                    }),
            )
            .width(width)
            .padding(0)
            .style(move |theme: &Theme, _| {
                let palette = theme.palette();
                button::Style {
                    text_color: if sorted {
                        palette.background.base.text
                    } else {
                        palette.background.base.text.scale_alpha(0.6)
                    },
                    ..button::Style::default()
                }
            })
            .on_press(Message::Manager(ManagerMessage::Sort(column)))
        };
        let thead = row![
            Space::new().width(30),
            heading("Tag", Column::Name, Length::Fill, false),
            heading("Notes", Column::Notes, Length::Fixed(80.0), true),
            heading("Last used", Column::Used, Length::Fixed(110.0), true),
            Space::new().width(150),
        ]
        .spacing(16)
        .padding([8, 12]);
        let mut table = column![];
        for (tag, count, used) in sorted(vault, tags.clone(), manager) {
            table = table
                .push(self.manager_row(manager, tag, count, used))
                .push(rule::horizontal(1).style(|theme: &Theme| rule::Style {
                    color: theme.palette().background.weak.color,
                    ..rule::default(theme)
                }));
        }
        let content = column![
            head,
            Space::new().height(16),
            column![].push(banner).push(thead).spacing(14),
            rule::horizontal(1),
            scrollable(table.padding(iced::Padding {
                // Room to scroll the last rows out from under the bar.
                bottom: if manager.selected.is_empty() {
                    0.0
                } else {
                    120.0
                },
                ..iced::Padding::ZERO
            }))
            .height(Length::Fill),
        ]
        .padding(iced::Padding {
            top: 26.0,
            right: 40.0,
            bottom: 0.0,
            left: 48.0,
        });
        let bulk = self.bulk(manager).map(|bulk| {
            container(bulk)
                .width(Length::Fill)
                .height(Length::Fill)
                .align_x(iced::Center)
                .align_y(iced::Bottom)
                .padding(iced::Padding {
                    bottom: 24.0,
                    ..iced::Padding::ZERO
                })
        });
        stack![content].push(bulk).into()
    }

    /// One tag's row: its box, name, count, last used and actions, with the
    /// rename field or delete question under it when open.
    fn manager_row<'a>(
        &'a self,
        manager: &'a Manager,
        tag: String,
        count: usize,
        used: String,
    ) -> Element<'a, Message> {
        let tag = tag.as_str();
        let on = manager.selected.contains(tag);
        let action = |label: &'static str, danger: bool, message: TagMessage| {
            button(text(label).size(12))
                .padding([4, 8])
                .style(move |theme: &Theme, status| {
                    let mut style = super::sidebar::choice(theme, status, false);
                    let palette = theme.palette();
                    style.text_color = if danger {
                        palette.danger.base.color
                    } else {
                        palette.background.base.text.scale_alpha(0.7)
                    };
                    style
                })
                .on_press(Message::Tag(message))
        };
        let pill = container(
            text(format!("#{tag}"))
                .size(13)
                .font(iced::Font {
                    weight: iced::font::Weight::Semibold,
                    ..iced::Font::new(livemark::fonts::PROSE)
                })
                .wrapping(Wrapping::None)
                .ellipsis(Ellipsis::End),
        )
        .padding([2, 9])
        .style(|theme: &Theme| container::Style {
            background: Some(theme.palette().primary.base.color.scale_alpha(0.15).into()),
            border: iced::Border::default().rounded(999),
            ..container::Style::default()
        });
        let owned = tag.to_owned();
        let line = container(
            row![
                container(checkbox(on).size(16).on_toggle(move |on| {
                    Message::Manager(ManagerMessage::Select(owned.clone(), on))
                }))
                .width(30),
                container(pill).width(Length::Fill),
                text(count.to_string())
                    .size(13)
                    .width(80)
                    .align_x(iced::alignment::Horizontal::Right),
                text(used)
                    .size(13)
                    .style(text::secondary)
                    .width(110)
                    .align_x(iced::alignment::Horizontal::Right),
                row![
                    action("Rename", false, TagMessage::StartRename(tag.to_owned())),
                    action("Delete", true, TagMessage::StartDelete(tag.to_owned())),
                ]
                .spacing(2)
                .width(150)
                .align_y(iced::Center),
            ]
            .spacing(16)
            .align_y(iced::Center),
        )
        .padding([6, 12])
        .style(move |theme: &Theme| container::Style {
            background: on.then(|| theme.palette().primary.base.color.scale_alpha(0.08).into()),
            ..container::Style::default()
        });
        let below = match &self.tag_action {
            Some(TagAction::Rename {
                tag: renaming,
                value,
            }) if renaming == tag => Some(self.rename_field(tag, value, count)),
            Some(TagAction::Delete(deleting)) if deleting == tag => Some(card(
                format!(
                    "Delete #{tag} from {}? In front matter it goes; in the text the word stays and loses its #.",
                    notes(count)
                ),
                "Delete",
                true,
            )),
            _ => None,
        };
        column![line]
            .push(below.map(|below| {
                container(container(below).width(Length::Fixed(560.0))).padding([6, 42])
            }))
            .into()
    }

    /// The bar for the selected tags, with the merge menu or the delete
    /// question over it.
    fn bulk<'a>(&'a self, manager: &'a Manager) -> Option<Element<'a, Message>> {
        let chosen = self.chosen();
        if chosen.is_empty() {
            return None;
        }
        let send = |message: ManagerMessage| Message::Manager(message);
        let above: Option<Element<'a, Message>> = if let Some(other) = &manager.merging {
            let count = |tag: &str| {
                self.vault.as_ref().map_or(0, |vault| {
                    vault
                        .notes
                        .iter()
                        .filter(|n| n.tags.iter().any(|t| t == tag))
                        .count()
                })
            };
            let mut menu = column![
                container(
                    text("Merge the selected tags into")
                        .size(12)
                        .style(text::secondary)
                )
                .padding([4, 10])
            ];
            for tag in &chosen {
                menu = menu.push(
                    button(
                        row![
                            text(format!("#{tag}")).size(14).width(Length::Fill),
                            text(notes(count(tag))).size(12).style(text::secondary),
                        ]
                        .align_y(iced::Center),
                    )
                    .width(Length::Fill)
                    .padding([6, 10])
                    .style(|theme: &Theme, status| super::sidebar::choice(theme, status, false))
                    .on_press(send(ManagerMessage::MergeInto(tag.clone()))),
                );
            }
            menu = menu.push(
                container(
                    text_input("Another tag...", other)
                        .id(ANOTHER)
                        .on_input(|other| Message::Manager(ManagerMessage::Another(other)))
                        .on_submit(send(ManagerMessage::MergeInto(other.clone())))
                        .size(14)
                        .padding([5, 8]),
                )
                .padding([4, 6]),
            );
            Some(
                container(menu.spacing(1))
                    .width(300)
                    .padding(4)
                    .style(container::bordered_box)
                    .into(),
            )
        } else if manager.deleting {
            let count = self.vault.as_ref().map_or(0, |vault| {
                vault
                    .notes
                    .iter()
                    .filter(|n| n.tags.iter().any(|t| chosen.contains(t)))
                    .count()
            });
            Some(
                container(
                    column![
                        text(format!(
                            "Delete {} from {}? In front matter they go; in the text the words stay and lose their #.",
                            listed(&chosen),
                            notes(count)
                        ))
                        .size(13),
                        row![
                            button(text("Delete").size(13))
                                .padding([5, 12])
                                .style(button::danger)
                                .on_press(send(ManagerMessage::ConfirmDelete)),
                            button(text("Cancel").size(13))
                                .padding([5, 12])
                                .style(button::secondary)
                                .on_press(send(ManagerMessage::Delete)),
                        ]
                        .spacing(6),
                    ]
                    .spacing(10),
                )
                .width(360)
                .padding(10)
                .style(container::bordered_box)
                .into(),
            )
        } else {
            None
        };
        let plain = |label: &'static str, solid: bool, danger: bool, message: ManagerMessage| {
            button(text(label).size(13))
                .padding([6, 12])
                .style(move |theme: &Theme, status| {
                    let palette = theme.palette();
                    let shade = match status {
                        button::Status::Hovered | button::Status::Pressed => 0.3,
                        _ if solid => 0.22,
                        _ => 0.0,
                    };
                    button::Style {
                        background: Some(
                            iced::Color::from_rgb(0.5, 0.5, 0.5)
                                .scale_alpha(shade)
                                .into(),
                        ),
                        text_color: if danger {
                            palette.danger.weak.color
                        } else {
                            palette.background.base.color
                        },
                        border: iced::Border::default().rounded(8),
                        ..button::Style::default()
                    }
                })
                .on_press(send(message))
        };
        let bar = container(
            row![
                text(format!("{} selected", plural(chosen.len(), "tag"))).size(13),
                Space::new().width(10),
                plain("Merge into...", true, false, ManagerMessage::Merging),
                plain("Delete", false, true, ManagerMessage::Delete),
                plain("×", false, false, ManagerMessage::Clear),
            ]
            .spacing(4)
            .align_y(iced::Center),
        )
        .padding(iced::Padding {
            top: 6.0,
            right: 6.0,
            bottom: 6.0,
            left: 16.0,
        })
        .style(|theme: &Theme| {
            let palette = theme.palette();
            container::Style {
                background: Some(palette.background.base.text.into()),
                text_color: Some(palette.background.base.color),
                border: iced::Border::default().rounded(12),
                ..container::Style::default()
            }
        });
        Some(
            column![]
                .push(above)
                .push(bar)
                .spacing(8)
                .align_x(iced::Center)
                .into(),
        )
    }
}
