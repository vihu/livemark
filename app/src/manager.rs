//! The tag manager (PLAN-005), in place of the note: every tag with how
//! many notes have it and when the newest of them was made, sortable;
//! rename and delete per row, as in the sidebar; several selected merged
//! into one tag or deleted, one Undo for them all; a look-alike pair of
//! tags suggested for merging.
use std::collections::{BTreeSet, HashMap};

use iced::Task;

use super::tag_actions::{clean, listed};
use super::tags::Edit;
use super::{App, Message};

/// A column the table sorts by.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Column {
    Name,
    Notes,
    Used,
}

#[derive(Debug)]
pub struct Manager {
    pub(super) sort: Column,
    pub(super) descending: bool,
    pub(super) selected: BTreeSet<String>,
    /// The merge menu, with the other tag typed so far, while it is open.
    pub(super) merging: Option<String>,
    /// The question before deleting the selected tags.
    pub(super) deleting: bool,
}

#[derive(Debug, Clone)]
pub enum ManagerMessage {
    /// Manage tags; from a tag's menu, with that tag selected.
    Open(Option<String>),
    Close,
    /// Sort by this column, or the other way when it is sorted by it.
    Sort(Column),
    Select(String, bool),
    Clear,
    /// The merge menu, open or closed again.
    Merging,
    Another(String),
    MergeInto(String),
    /// The question before deleting, open or closed again.
    Delete,
    ConfirmDelete,
    /// The look-alike pair: the first merged into the second, or both kept.
    Accept(String, String),
    KeepBoth(String, String),
}

/// The other tag field in the merge menu.
pub(super) const ANOTHER: iced::widget::Id = iced::widget::Id::new("livemark-merge-into");

impl App {
    pub(crate) fn manager_update(&mut self, message: ManagerMessage) -> Task<Message> {
        match message {
            ManagerMessage::Open(tag) => {
                self.tag_menu = None;
                self.tag_action = None;
                self.manager = Some(Manager {
                    sort: Column::Notes,
                    descending: true,
                    selected: tag.into_iter().collect(),
                    merging: None,
                    deleting: false,
                });
            }
            ManagerMessage::Close => {
                self.manager = None;
                self.tag_action = None;
                return livemark::widget::Editor::focus();
            }
            ManagerMessage::Accept(from, into) => {
                let said = format!("Merged #{from} into #{into}");
                self.run_edits(vec![Edit::Rename { from, to: into }], said);
            }
            ManagerMessage::KeepBoth(a, b) => self.kept_apart.push((a, b)),
            ManagerMessage::MergeInto(into) => {
                let into = clean(&into);
                let from: Vec<String> = self.chosen().into_iter().filter(|t| *t != into).collect();
                if into.is_empty() || from.is_empty() {
                    return Task::none();
                }
                let said = format!("Merged {} into #{into}", listed(&from));
                let edits = from
                    .into_iter()
                    .map(|from| Edit::Rename {
                        from,
                        to: into.clone(),
                    })
                    .collect();
                if self.run_edits(edits, said) {
                    return self.manager_update(ManagerMessage::Clear);
                }
            }
            ManagerMessage::ConfirmDelete => {
                let tags = self.chosen();
                let said = format!("Deleted {}", listed(&tags));
                if self.run_edits(tags.into_iter().map(Edit::Delete).collect(), said) {
                    return self.manager_update(ManagerMessage::Clear);
                }
            }
            message => {
                let Some(manager) = &mut self.manager else {
                    return Task::none();
                };
                match message {
                    ManagerMessage::Sort(column) if column == manager.sort => {
                        manager.descending = !manager.descending;
                    }
                    ManagerMessage::Sort(column) => {
                        manager.sort = column;
                        manager.descending = column != Column::Name;
                    }
                    ManagerMessage::Select(tag, on) => {
                        if on {
                            manager.selected.insert(tag);
                        } else {
                            manager.selected.remove(&tag);
                        }
                        manager.merging = None;
                        manager.deleting = false;
                    }
                    ManagerMessage::Clear => {
                        manager.selected.clear();
                        manager.merging = None;
                        manager.deleting = false;
                    }
                    ManagerMessage::Merging => {
                        manager.deleting = false;
                        manager.merging = match manager.merging {
                            Some(_) => None,
                            None => Some(String::new()),
                        };
                        return iced::widget::operation::focus(ANOTHER);
                    }
                    ManagerMessage::Another(other) => manager.merging = Some(other),
                    ManagerMessage::Delete => {
                        manager.merging = None;
                        manager.deleting = !manager.deleting;
                    }
                    _ => {}
                }
            }
        }
        Task::none()
    }

    /// The selected tags the vault still has.
    pub(super) fn chosen(&self) -> Vec<String> {
        let (Some(manager), Some(vault)) = (&self.manager, &self.vault) else {
            return Vec::new();
        };
        let tags: BTreeSet<String> = vault.tags().into_iter().map(|(tag, _)| tag).collect();
        manager.selected.intersection(&tags).cloned().collect()
    }

    /// The pair of tags the manager suggests merging, the smaller first.
    pub(crate) fn suggestion(&self) -> Option<(String, String)> {
        look_alike(&self.vault.as_ref()?.tags(), &self.kept_apart)
    }
}

/// The tags with their counts and newest dates, in the manager's order.
pub(super) fn sorted(
    vault: &super::vault::Vault,
    tags: Vec<(String, usize)>,
    manager: &Manager,
) -> Vec<(String, usize, String)> {
    let mut used: HashMap<&str, &str> = HashMap::new();
    for note in &vault.notes {
        let Some(created) = note.created.as_deref() else {
            continue;
        };
        for tag in &note.tags {
            let newest = used.entry(tag).or_default();
            *newest = (*newest).max(created);
        }
    }
    let mut rows: Vec<(String, usize, String)> = tags
        .into_iter()
        .map(|(tag, count)| {
            let date = used
                .get(tag.as_str())
                .copied()
                .unwrap_or_default()
                .to_owned();
            (tag, count, date)
        })
        .collect();
    match manager.sort {
        Column::Name => rows.sort_by(|a, b| a.0.cmp(&b.0)),
        Column::Notes => rows.sort_by(|a, b| a.1.cmp(&b.1).then(b.0.cmp(&a.0))),
        Column::Used => rows.sort_by(|a, b| a.2.cmp(&b.2).then(b.0.cmp(&a.0))),
    }
    if manager.descending {
        rows.reverse();
    }
    rows
}

pub(super) fn plural(n: usize, what: &str) -> String {
    if n == 1 {
        format!("1 {what}")
    } else {
        format!("{n} {what}s")
    }
}

/// The first pair of tags that look alike (most used first), the less
/// used one first, unless kept apart: the same but for `-` and `_`, one
/// the other's plural, or a letter apart in a name of five or more (two in
/// one of eight or more), their digits the same (`q1` is not `q2`).
pub fn look_alike(tags: &[(String, usize)], kept: &[(String, String)]) -> Option<(String, String)> {
    for (i, (a, _)) in tags.iter().enumerate() {
        for (b, _) in &tags[i + 1..] {
            let apart = kept
                .iter()
                .any(|(x, y)| (x == a && y == b) || (x == b && y == a));
            if !apart && alike(a, b) {
                return Some((b.clone(), a.clone()));
            }
        }
    }
    None
}

fn alike(a: &str, b: &str) -> bool {
    let bare = |s: &str| s.replace(['-', '_'], "");
    if bare(a) == bare(b) {
        return true;
    }
    let plural = |x: &str, y: &str| y.strip_prefix(x).is_some_and(|s| s == "s" || s == "es");
    if plural(a, b) || plural(b, a) {
        return true;
    }
    let digits = |s: &str| s.chars().filter(char::is_ascii_digit).collect::<String>();
    let most = match a.chars().count().min(b.chars().count()) {
        0..=4 => return false,
        5..=7 => 1,
        _ => 2,
    };
    digits(a) == digits(b) && within(a, b, most)
}

/// Whether `a` and `b` are at most `most` letters added, dropped or
/// changed apart.
fn within(a: &str, b: &str, most: usize) -> bool {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    if a.len().abs_diff(b.len()) > most {
        return false;
    }
    let mut before: Vec<usize> = (0..=b.len()).collect();
    for (i, x) in a.iter().enumerate() {
        let mut now = vec![i + 1];
        for (j, y) in b.iter().enumerate() {
            let step = (before[j] + usize::from(x != y))
                .min(before[j + 1] + 1)
                .min(now[j] + 1);
            now.push(step);
        }
        before = now;
    }
    before[b.len()] <= most
}

#[cfg(test)]
mod tests {
    use super::look_alike;

    fn tags(names: &[&str]) -> Vec<(String, usize)> {
        names
            .iter()
            .enumerate()
            .map(|(i, n)| (n.to_string(), 10 - i))
            .collect()
    }

    #[test]
    fn look_alike_tags_are_spelling_slips_plurals_and_dashes_not_numbers() {
        let pair = |names: &[&str]| look_alike(&tags(names), &[]);
        let some = |a: &str, b: &str| Some((a.to_owned(), b.to_owned()));
        assert_eq!(
            pair(&["travel", "work", "trip", "trips"]),
            some("trips", "trip")
        );
        assert_eq!(pair(&["to-do", "todo"]), some("todo", "to-do"));
        assert_eq!(pair(&["journal", "jornal"]), some("jornal", "journal"));
        assert_eq!(pair(&["meetings", "meeting"]), some("meeting", "meetings"));
        assert_eq!(
            pair(&["reading-list", "readng-lst"]),
            some("readng-lst", "reading-list")
        );
        // Short names and numbered ones are apart.
        assert_eq!(pair(&["work", "word", "home", "hope"]), None);
        assert_eq!(pair(&["2025-q1", "2025-q2", "week-1", "week-2"]), None);
        assert_eq!(pair(&["travel", "trips"]), None);
        // Kept apart, either way round.
        let kept = [("trips".to_owned(), "trip".to_owned())];
        assert_eq!(look_alike(&tags(&["trip", "trips"]), &kept), None);
    }
}
