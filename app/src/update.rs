//! What the app does with each message (PLAN-002 onwards); the parts with
//! their own modules hand over to them.
use iced::Task;

use super::{After, App, Message, file, open_link, sidebar, tag_actions};

impl App {
    pub(crate) fn update(&mut self, message: Message) -> Task<Message> {
        // A File menu item picked.
        if matches!(
            message,
            Message::New | Message::Open | Message::Save { .. } | Message::Recent(_)
        ) {
            self.menu = false;
        }
        match message {
            // The note is out of sight behind the tag manager: its toolbar
            // does nothing meanwhile.
            Message::Editor(_) if self.manager.is_some() || self.appearance => {}
            Message::Editor(message) if message.pasted_image().is_some() => {
                return self.paste_picture(message.pasted_image().unwrap_or_default());
            }
            Message::Dropped(file) => return self.dropped(file),
            Message::Vault(message) => return self.vault_update(message),
            Message::Tag(message) => return self.tag_update(message),
            Message::Manager(message) => return self.manager_update(message),
            Message::Note(message) => return self.note_update(message),
            Message::Search(message) => return self.search_update(message),
            Message::Editor(message) => {
                if let Some(tag) = message.tag() {
                    self.show_tag(tag);
                }
                // A link to a note in the vault opens here.
                if let Some(path) = message.link().and_then(|dest| self.note_link(dest)) {
                    return self.update(Message::Opened(Some(path)));
                }
                // Only a failure is reported; an open or save error stays.
                if let Some(Err(error)) = message.link().map(open_link) {
                    self.error = Some(error);
                }
                let task = self.editor.update(message).map(Message::Editor);
                self.load_images();
                self.remember_zoom();
                self.offer_choices();
                return task;
            }
            // In a vault a new note is named first and made there.
            Message::New if self.vault.is_some() => {
                return self.vault_update(sidebar::VaultMessage::NewNote);
            }
            Message::New if self.unsaved() => self.pending = Some(After::New),
            Message::New => return self.new_note(),
            Message::Open if self.unsaved() => self.pending = Some(After::Open),
            Message::Open => return Task::perform(file::pick(), Message::Opened),
            // Typing while the dialog was open is not dropped unasked.
            Message::Opened(Some(path))
                if self.unsaved() && self.discarded != Some(self.editor.version()) =>
            {
                self.pending = Some(After::Load(path));
            }
            Message::Opened(Some(path)) => return self.load(path),
            Message::Opened(None) => {}
            // Never over a file changed on disk since it was opened or
            // saved: ask first (Load it, Keep mine), then save again.
            Message::Save { choose: false } if self.changed_on_disk() => self.changed = true,
            Message::Save { choose } => {
                let text = self.editor.text().to_owned();
                let version = self.editor.version();
                let path = self.path.clone().filter(|_| !choose);
                return Task::perform(file::save_as(path, text), move |result| {
                    Message::Saved(result, version)
                });
            }
            Message::Saved(Ok(Some(path)), version) => {
                self.error = None;
                self.stamp = file::modified(&path);
                self.changed = false;
                // Saved somewhere new: its images are looked for there.
                if self.path.as_ref() != Some(&path) {
                    self.tried.clear();
                }
                self.settings.opened(&path);
                self.remember();
                self.path = Some(path);
                self.saved = version;
                self.load_images();
                self.refresh_vault();
                // A picture pasted before the note had a place.
                if let Some(png) = self.waiting_picture.take() {
                    let _ = self.paste_picture(png);
                }
                // Typing while the save dialog was open: ask again.
                if let Some(after) = self.pending.take() {
                    if self.unsaved() {
                        self.pending = Some(after);
                    } else {
                        return self.carry_on(after);
                    }
                }
            }
            Message::Saved(Ok(None), _) => self.pending = None,
            Message::Saved(Err(error), _) => {
                self.error = Some(error);
                self.pending = None;
            }
            Message::CloseRequested if self.unsaved() => self.pending = Some(After::Close),
            Message::CloseRequested => return self.exit(),
            Message::Unsaved(Some(true)) => return self.update(Message::Save { choose: false }),
            Message::Unsaved(Some(false)) => {
                self.discarded = Some(self.editor.version());
                if let Some(after) = self.pending.take() {
                    return self.carry_on(after);
                }
            }
            Message::Unsaved(None) => self.pending = None,
            Message::Focused => {
                self.check_disk();
                self.refresh_vault();
            }
            Message::Resized(size) => self.settings.window = Some((size.width, size.height)),
            Message::Reload(true) => {
                // Nothing is unsaved after it: a close or open waiting on
                // the unsaved text is asked for again.
                self.pending = None;
                self.reload();
            }
            Message::Zoom(step) => {
                let tenths = (self.editor.zoom() * 10.0).round() + f32::from(step);
                self.editor
                    .set_zoom(if step == 0 { 1.0 } else { tenths / 10.0 });
                self.remember_zoom();
            }
            Message::Menu(open) => {
                self.menu = open;
                // Notes moved or deleted since are not offered.
                let count = self.settings.recent.len();
                self.settings.recent.retain(|path| path.exists());
                if self.settings.recent.len() != count {
                    self.remember();
                }
            }
            Message::Recent(path) if !path.exists() => {
                self.error = Some(format!("No longer there: {}", path.display()));
                self.settings.recent.retain(|recent| *recent != path);
                self.remember();
            }
            Message::Recent(path) => return self.update(Message::Opened(Some(path))),
            Message::Resize(resize) => self.resize_update(resize),
            Message::Undo => {
                self.toast = None;
                match self.undo_last() {
                    Ok(0) => {}
                    Ok(kept) => {
                        self.error = Some(format!(
                            "{} changed since were kept as they are",
                            tag_actions::notes(kept)
                        ))
                    }
                    Err(error) => self.error = Some(error),
                }
            }
            Message::DismissToast => self.toast = None,
            Message::Escape => {
                self.tag_menu = None;
                self.tag_action = None;
                self.note_menu = None;
                self.note_action = None;
            }
            Message::Sidebar => {
                self.settings.sidebar = !self.settings.sidebar;
                self.remember();
            }
            Message::Appearance(message) => return self.appearance_update(message),
            Message::Reload(false) => {
                // Keep this text; saving will replace the file's.
                self.changed = false;
                self.stamp = self.path.as_deref().and_then(file::modified);
            }
        }
        Task::none()
    }
}
