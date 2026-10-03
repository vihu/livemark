//! Autosave (PLAN-006), on unless turned off in Appearance: a note with a
//! file is written two seconds after typing stops, when the window loses
//! focus, and before another note opens, the window closes or a change
//! across the vault needs it saved, as Ctrl+S writes it. Never over a file
//! changed on disk (that asks, as before); a new note without a file still
//! asks where. One `async_io` timer at a time waits for the quiet, so no
//! thread is held while it does.
use std::time::{Duration, Instant};

use iced::Task;

use super::{App, Message, file};

/// How long typing stops before the note is written.
pub(crate) const QUIET: Duration = Duration::from_secs(2);

impl App {
    /// After an edit: its time noted, and a wait started unless one is
    /// under way.
    pub(crate) fn typed(&mut self) -> Task<Message> {
        if !self.settings.autosave || self.path.is_none() {
            return Task::none();
        }
        self.last_edit = Some(Instant::now());
        if self.autosave_waiting {
            return Task::none();
        }
        self.autosave_waiting = true;
        wait(QUIET)
    }

    /// The wait over: the note written when typing stopped long enough,
    /// else a wait for the rest.
    pub(crate) fn autosave_tick(&mut self) -> Task<Message> {
        self.autosave_waiting = false;
        let Some(at) = self.last_edit else {
            return Task::none();
        };
        let quiet = at.elapsed();
        if quiet < QUIET {
            self.autosave_waiting = true;
            return wait(QUIET - quiet);
        }
        self.save_quietly();
        Task::none()
    }

    /// Writes the note now when autosave may: on, edits unsaved, a file
    /// for them, nothing asked meanwhile, the file as it was loaded or
    /// saved. Whether it wrote.
    pub(crate) fn save_quietly(&mut self) -> bool {
        if !self.settings.autosave || !self.unsaved() || self.pending.is_some() || self.changed {
            return false;
        }
        let Some(path) = self.path.clone() else {
            return false;
        };
        if self.changed_on_disk() {
            return false;
        }
        match file::save(&path, self.editor.text()) {
            Ok(()) => {
                self.saved = self.editor.version();
                self.stamp = file::modified(&path);
                self.last_edit = None;
                self.refresh_vault();
                true
            }
            Err(error) => {
                self.error = Some(format!("{}: {error}", path.display()));
                false
            }
        }
    }
}

/// A message after `duration`.
fn wait(duration: Duration) -> Task<Message> {
    Task::perform(async_io::Timer::after(duration), |_| Message::AutosaveTick)
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    #[test]
    fn the_wait_ends_on_an_executor_that_is_not_async_ios() {
        // iced's thread pool polls the timer; async-io's own thread wakes it.
        let start = Instant::now();
        iced::futures::executor::block_on(async_io::Timer::after(Duration::from_millis(30)));
        assert!(start.elapsed() >= Duration::from_millis(30));
    }
}
