//! What can be done to a tag from the sidebar (PLAN-005): its menu, a
//! rename in place that says when it is a merge, a delete that says how
//! many notes, and the Undo after either.
use iced::widget::text::{Ellipsis, Wrapping};
use iced::widget::{button, column, container, mouse_area, row, text, text_input};
use iced::{Element, Length, Task, Theme};

use super::sidebar::{Shown, choice};
use super::tags::Edit;
use super::{App, Message};

/// The rename field, focused when it opens.
const RENAME: iced::widget::Id = iced::widget::Id::new("livemark-tag-rename");

#[derive(Debug, Clone)]
pub enum TagMessage {
    /// The pointer over a tag's row, or off them all.
    Hover(Option<String>),
    /// Its menu, open or closed again.
    Menu(String),
    StartRename(String),
    RenameText(String),
    StartDelete(String),
    /// Rename (or merge), or delete, as the open question says.
    Confirm,
    Cancel,
    /// F2: rename the tag the notes are filtered by.
    RenameShown,
    /// The tag manager, with this tag selected.
    Manage(String),
    Undo,
    /// The Undo note closed.
    Dismiss,
}

/// The question open under a tag.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TagAction {
    Rename { tag: String, value: String },
    Delete(String),
}

/// A typed tag name as the vault keeps it.
pub fn clean(name: &str) -> String {
    name.trim()
        .trim_start_matches('#')
        .trim()
        .to_lowercase()
        .replace(char::is_whitespace, "-")
}

pub(crate) fn notes(n: usize) -> String {
    if n == 1 {
        "1 note".into()
    } else {
        format!("{n} notes")
    }
}

/// `#a`, `#a and #b`, `#a, #b and #c`.
pub(crate) fn listed(tags: &[String]) -> String {
    let tags: Vec<String> = tags.iter().map(|tag| format!("#{tag}")).collect();
    match tags.split_last() {
        Some((last, [])) => last.clone(),
        Some((last, rest)) => format!("{} and {last}", rest.join(", ")),
        None => String::new(),
    }
}

impl App {
    pub(crate) fn tag_update(&mut self, message: TagMessage) -> Task<Message> {
        match message {
            TagMessage::Hover(tag) => self.hovered_tag = tag,
            TagMessage::Menu(tag) => {
                self.tag_action = None;
                self.tag_menu = (self.tag_menu.as_ref() != Some(&tag)).then_some(tag);
            }
            TagMessage::StartRename(tag) => {
                self.tag_menu = None;
                self.tag_action = Some(TagAction::Rename {
                    value: tag.clone(),
                    tag,
                });
                return iced::widget::operation::focus(RENAME);
            }
            TagMessage::RenameShown => {
                if let Shown::Tag(tag) = self.shown.clone() {
                    return self.tag_update(TagMessage::StartRename(tag));
                }
            }
            TagMessage::RenameText(value) => {
                if let Some(TagAction::Rename { value: v, .. }) = &mut self.tag_action {
                    *v = value;
                }
            }
            TagMessage::StartDelete(tag) => {
                self.tag_menu = None;
                self.tag_action = Some(TagAction::Delete(tag));
            }
            TagMessage::Cancel => {
                self.tag_menu = None;
                self.tag_action = None;
            }
            TagMessage::Confirm => {
                let Some(action) = self.tag_action.take() else {
                    return Task::none();
                };
                let (edit, said) = match action {
                    TagAction::Rename { tag, value } => {
                        let to = clean(&value);
                        if to.is_empty() || to == tag {
                            return Task::none();
                        }
                        let merge = self.tag_count(&to) > 0;
                        let said = if merge {
                            format!("Merged #{tag} into #{to}")
                        } else {
                            format!("Renamed #{tag} to #{to}")
                        };
                        (Edit::Rename { from: tag, to }, said)
                    }
                    TagAction::Delete(tag) => {
                        (Edit::Delete(tag.clone()), format!("Deleted #{tag}"))
                    }
                };
                self.run_edits(vec![edit], said);
            }
            TagMessage::Undo => {
                self.toast = None;
                match self.undo_tags() {
                    Ok(0) => {}
                    Ok(kept) => {
                        self.error = Some(format!(
                            "{} changed since were kept as they are",
                            notes(kept)
                        ))
                    }
                    Err(error) => self.error = Some(error),
                }
            }
            TagMessage::Dismiss => self.toast = None,
            TagMessage::Manage(tag) => {
                return self.manager_update(super::manager::ManagerMessage::Open(Some(tag)));
            }
        }
        Task::none()
    }

    /// Carries out `edits` as one, then says `said` and how many notes
    /// changed over the Undo; whether it went through.
    pub(crate) fn run_edits(&mut self, edits: Vec<Edit>, said: String) -> bool {
        match self.edit_tags(edits) {
            Ok(n) => {
                self.toast = Some(format!("{said}: {} changed", notes(n)));
                self.error = None;
                true
            }
            Err(error) => {
                self.error = Some(error);
                false
            }
        }
    }

    /// How many notes have `tag`.
    fn tag_count(&self, tag: &str) -> usize {
        self.vault.as_ref().map_or(0, |vault| {
            vault
                .notes
                .iter()
                .filter(|n| n.tags.iter().any(|t| t == tag))
                .count()
        })
    }

    /// One tag's row in the sidebar, with its menu, rename field or delete
    /// question under it when open.
    pub(crate) fn tag_row(&self, tag: &str, count: usize) -> Element<'_, Message> {
        let asking = self.manager.is_none();
        if let Some(TagAction::Rename {
            tag: renaming,
            value,
        }) = &self.tag_action
            && renaming == tag
            && asking
        {
            return self.rename_field(tag, value, count);
        }
        let on = self.shown == Shown::Tag(tag.to_owned());
        let open = self.tag_menu.as_deref() == Some(tag);
        let hovered = self.hovered_tag.as_deref() == Some(tag);
        let name = button(
            row![
                text("#").size(14).width(16).style(text::secondary),
                text(tag.to_owned())
                    .size(14)
                    .wrapping(Wrapping::None)
                    .ellipsis(Ellipsis::End),
            ]
            .spacing(4),
        )
        .width(Length::Fill)
        .padding([5, 0])
        .style(|theme: &Theme, _| button::Style {
            text_color: theme.palette().background.base.text,
            ..button::Style::default()
        })
        .on_press(Message::Vault(super::sidebar::VaultMessage::Show(
            Shown::Tag(tag.to_owned()),
        )));
        let more = button(text("···").size(13))
            .padding([2, 6])
            .style(move |theme: &Theme, status| {
                let mut style = choice(theme, status, false);
                if !(on || open || hovered) {
                    style.text_color = iced::Color::TRANSPARENT;
                }
                if open {
                    style.background = Some(theme.palette().background.strong.color.into());
                }
                style
            })
            .on_press(Message::Tag(TagMessage::Menu(tag.to_owned())));
        let line = container(
            row![
                name,
                text(count.to_string()).size(12).style(text::secondary),
                more
            ]
            .spacing(6)
            .align_y(iced::Center),
        )
        .padding([0, 4])
        .style(move |theme: &Theme| container::Style {
            background: (on || hovered).then(|| {
                let palette = theme.palette();
                if on {
                    palette.primary.weak.color
                } else {
                    palette.background.weak.color
                }
                .into()
            }),
            border: iced::Border::default().rounded(5),
            ..container::Style::default()
        });
        let line = mouse_area(line)
            .on_enter(Message::Tag(TagMessage::Hover(Some(tag.to_owned()))))
            .on_exit(Message::Tag(TagMessage::Hover(None)));
        let below: Option<Element<'_, Message>> = if open {
            Some(self.tag_menu_view(tag))
        } else if asking && self.tag_action == Some(TagAction::Delete(tag.to_owned())) {
            Some(card(
                format!(
                    "Delete #{tag} from {}? In front matter it goes; in the text the word stays and loses its #.",
                    notes(count)
                ),
                "Delete",
                true,
            ))
        } else {
            None
        };
        column![line].push(below).spacing(2).into()
    }

    fn tag_menu_view(&self, tag: &str) -> Element<'_, Message> {
        let item = |label: &'static str, hint: &'static str, danger: bool, message: TagMessage| {
            button(
                row![
                    text(label).size(13).width(Length::Fill),
                    text(hint).size(11).style(text::secondary),
                ]
                .align_y(iced::Center),
            )
            .width(Length::Fill)
            .padding([6, 10])
            .style(move |theme: &Theme, status| {
                let mut style = choice(theme, status, false);
                if danger {
                    style.text_color = theme.palette().danger.base.color;
                }
                style
            })
            .on_press(Message::Tag(message))
        };
        container(
            column![
                item(
                    "Rename or merge",
                    "F2",
                    false,
                    TagMessage::StartRename(tag.to_owned())
                ),
                item(
                    "Open in the tag manager",
                    "",
                    false,
                    TagMessage::Manage(tag.to_owned())
                ),
                item(
                    "Delete from every note",
                    "",
                    true,
                    TagMessage::StartDelete(tag.to_owned())
                ),
            ]
            .spacing(1),
        )
        .padding(4)
        .style(container::bordered_box)
        .into()
    }

    pub(crate) fn rename_field<'a>(
        &'a self,
        tag: &str,
        value: &'a str,
        count: usize,
    ) -> Element<'a, Message> {
        let to = clean(value);
        let (said, action) = if to.is_empty() || to == tag {
            (
                "Type a new name; a name that exists merges the two.".to_owned(),
                "Rename".to_owned(),
            )
        } else if self.tag_count(&to) > 0 {
            (
                format!(
                    "#{to} exists already, so #{tag} merges into it: {} {}, in front matter and in the text.",
                    notes(count),
                    if count == 1 { "changes" } else { "change" }
                ),
                format!("Merge into #{to}"),
            )
        } else {
            (
                format!("#{tag} becomes #{to} in {}.", notes(count)),
                "Rename".to_owned(),
            )
        };
        column![
            text_input("New name", value)
                .id(RENAME)
                .on_input(|value| Message::Tag(TagMessage::RenameText(value)))
                .on_submit(Message::Tag(TagMessage::Confirm))
                .size(14)
                .padding([4, 8]),
            card(said, &action, false),
        ]
        .spacing(4)
        .into()
    }

    /// The note an edit across the vault leaves, with its Undo.
    pub(crate) fn undo_toast(&self) -> Option<Element<'_, Message>> {
        let said = self.toast.as_ref().filter(|_| self.undo.is_some())?;
        Some(
            container(
                row![
                    text(said.as_str()).size(13),
                    button(text("Undo").size(13))
                        .padding([4, 10])
                        .on_press(Message::Tag(TagMessage::Undo)),
                    button(text("×").size(14))
                        .padding([2, 8])
                        .style(|theme: &Theme, _| button::Style {
                            text_color: theme.palette().background.base.color,
                            ..button::Style::default()
                        })
                        .on_press(Message::Tag(TagMessage::Dismiss)),
                ]
                .spacing(12)
                .align_y(iced::Center),
            )
            .padding([8, 10])
            .style(|theme: &Theme| {
                let palette = theme.palette();
                container::Style {
                    background: Some(palette.background.base.text.into()),
                    text_color: Some(palette.background.base.color),
                    border: iced::Border::default().rounded(10),
                    ..container::Style::default()
                }
            })
            .into(),
        )
    }
}

/// A question under a tag: what will happen, and the buttons.
pub(crate) fn card<'a>(said: String, action: &str, danger: bool) -> Element<'a, Message> {
    container(
        column![
            text(said).size(13),
            row![
                button(text(action.to_owned()).size(13))
                    .padding([5, 12])
                    .style(if danger {
                        button::danger
                    } else {
                        button::primary
                    })
                    .on_press(Message::Tag(TagMessage::Confirm)),
                button(text("Cancel").size(13))
                    .padding([5, 12])
                    .style(button::secondary)
                    .on_press(Message::Tag(TagMessage::Cancel)),
            ]
            .spacing(6),
        ]
        .spacing(10),
    )
    .padding(10)
    .style(container::bordered_box)
    .into()
}
