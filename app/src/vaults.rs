//! Changing vaults (PLAN-006), one way whatever starts it: the vault menu's
//! recent vaults, Open vault, or a note opened from a vault other than the
//! open one. The vault comes in with nothing of the last one left over
//! (filters, lists, questions, the tag manager, Undo, which would write
//! the other vault's files), and opens the note last open in it, else its
//! newest (an empty vault keeps the note open, to be moved in). Close
//! vault leaves the note open as a plain file.
use std::path::{Path, PathBuf};

use iced::Task;

use super::sidebar::Shown;
use super::vault::Vault;
use super::{App, Message};

/// How many vaults the menu remembers.
const VAULTS: usize = 6;

impl App {
    /// The vault at `root` in place of the open one, and its note open.
    pub(crate) fn switch_vault(&mut self, root: &Path) -> Task<Message> {
        if let Err(error) = self.enter_vault(root) {
            self.error = Some(error);
            return Task::none();
        }
        let Some(vault) = &self.vault else {
            return Task::none();
        };
        let root = vault.root.clone();
        let next = self
            .settings
            .recent
            .iter()
            .find(|path| path.starts_with(&root) && path.exists())
            .cloned()
            .or_else(|| vault.notes.first().map(|note| note.path.clone()));
        match next {
            Some(path) if self.path.as_ref() != Some(&path) => {
                self.update(Message::Opened(Some(path)))
            }
            // An empty vault keeps the note open, to be moved into it.
            _ => Task::none(),
        }
    }

    /// The vault at `root` made the open one, fresh, and remembered first
    /// among the recent vaults; its note left to the caller.
    pub(crate) fn enter_vault(&mut self, root: &Path) -> Result<(), String> {
        let vault = Vault::open(root).map_err(|error| {
            self.settings.vaults.retain(|known| known != root);
            self.remember();
            format!("{}: {error}", root.display())
        })?;
        self.leave_vault_state();
        let root = vault.root.clone();
        self.settings.vault = Some(root.clone());
        self.settings.vaults.retain(|known| *known != root);
        self.settings.vaults.insert(0, root);
        self.settings.vaults.truncate(VAULTS);
        self.remember();
        self.vault = Some(vault);
        self.refresh_footer();
        Ok(())
    }

    /// No vault open: the note stays, a plain file.
    pub(crate) fn close_vault(&mut self) {
        self.leave_vault_state();
        self.vault = None;
        self.settings.vault = None;
        self.remember();
        self.refresh_footer();
    }

    /// What belonged to the vault left: filters, lists, questions, the tag
    /// manager, the last change's Undo.
    fn leave_vault_state(&mut self) {
        self.shown = Shown::All;
        self.all_tags = false;
        self.listing = None;
        self.search = None;
        self.manager = None;
        self.naming = None;
        self.tag_action = None;
        self.note_action = None;
        self.hovered_tag = None;
        self.context = None;
        self.undo = None;
        self.toast = None;
        self.kept_apart.clear();
        self.outside_dismissed = None;
    }

    /// The remembered vault, not the open one, that holds `path` (the
    /// deepest, should one hold another).
    /// Only while a vault is open: a note opened with none open stays a
    /// plain file.
    pub(crate) fn vault_holding(&self, path: &Path) -> Option<PathBuf> {
        let open = self.vault.as_ref()?;
        if !self.outside_vault_of(open, path) {
            return None;
        }
        let path = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        self.settings
            .vaults
            .iter()
            .filter(|root| path.starts_with(root) && root.is_dir())
            .max_by_key(|root| root.components().count())
            .cloned()
    }

    fn outside_vault_of(&self, vault: &Vault, path: &Path) -> bool {
        if path.starts_with(&vault.root) {
            return false;
        }
        let path = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        !path.starts_with(&vault.root)
    }
}
