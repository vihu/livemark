//! Keys pressed in the surface: the find bar's, the completion list's,
//! Tab, then shortcuts handled before iced's text bindings (formatting,
//! REFERENCE-001 section 12; source mode, section 17; the document's start
//! and end, section 13; the line commands of CodeMirror's default keymap,
//! `edit::lines`; Ctrl/Cmd+T for a new tag, PLAN-005), then the bindings.
//! Letters are read from the key whatever the layout; with Alt they are
//! left alone, so AltGr letters still type.
use std::time::Instant;

use iced::advanced::{Shell, clipboard};
use iced::keyboard::{self, Modifiers, key::Named};
use iced::widget::text_editor::{Binding, KeyPress, Motion as IcedMotion};

use super::find::FindInput;
use super::surface::{State, Surface};
use super::{Input, Key, Message, Pane, Vertical};
use crate::edit::Motion;
use crate::edit::format::Format;

/// The shortcut `key` with `modifiers` is, if any.
pub(super) fn shortcut(
    key: &keyboard::Key,
    physical_key: keyboard::key::Physical,
    modifiers: Modifiers,
) -> Option<Key> {
    let letter = key.to_latin(physical_key);
    let command = modifiers.command() && !modifiers.alt();
    let document = |end: bool| {
        let motion = if end {
            Motion::DocumentEnd
        } else {
            Motion::DocumentStart
        };
        Some(Key::Move(motion, modifiers.shift()))
    };
    match key.as_ref() {
        // Ctrl/Cmd+Home and End: the document's start and end. iced's
        // bindings take Cmd+Home and End on macOS for the line's.
        keyboard::Key::Named(named @ (Named::Home | Named::End)) if command => {
            document(named == Named::End)
        }
        // Cmd+Up and Cmd+Down too on macOS, as there (CodeMirror's
        // standard keymap).
        keyboard::Key::Named(named @ (Named::ArrowUp | Named::ArrowDown))
            if modifiers.macos_command() && !modifiers.alt() =>
        {
            document(named == Named::ArrowDown)
        }
        // Alt+Up and Alt+Down move the lines, with Shift copy them.
        keyboard::Key::Named(named @ (Named::ArrowUp | Named::ArrowDown))
            if modifiers.alt() && !modifiers.command() =>
        {
            let down = named == Named::ArrowDown;
            Some(if modifiers.shift() {
                Key::CopyLines(down)
            } else {
                Key::MoveLines(down)
            })
        }
        keyboard::Key::Named(Named::Enter) if command && !modifiers.shift() => Some(Key::BlankLine),
        // Alt+Enter follows the link at the caret (REFERENCE-001 section 5).
        keyboard::Key::Named(Named::Enter)
            if modifiers.alt() && !modifiers.command() && !modifiers.shift() =>
        {
            Some(Key::Follow)
        }
        _ if command && modifiers.shift() => match letter? {
            // The user's pick.
            'e' => Some(Key::ToggleMode),
            'k' => Some(Key::DeleteLines),
            _ => None,
        },
        _ if command => match letter? {
            'b' => Some(Key::Format(Format::Bold)),
            'i' => Some(Key::Format(Format::Italic)),
            'e' => Some(Key::Format(Format::Code)),
            'k' => Some(Key::Link),
            't' => Some(Key::AddTag),
            _ => None,
        },
        _ => None,
    }
}

impl Surface<'_> {
    /// A key pressed with the text focused or the find bar open: the find
    /// bar's keys first (also while its fields have focus), then the
    /// completion list's, Tab, the shortcuts, then iced's text bindings.
    pub(super) fn key_pressed(
        &self,
        event: &keyboard::Event,
        state: &mut State,
        shell: &mut Shell<'_, Message>,
    ) {
        let keyboard::Event::KeyPressed {
            key,
            modified_key,
            physical_key,
            modifiers,
            text,
            ..
        } = event
        else {
            return;
        };
        let publish = |shell: &mut Shell<'_, Message>, input| shell.publish(Message(input));
        // The find bar's keys, also while its fields have focus
        // (REFERENCE-001 section 16).
        if let Some(input) = find_key(self.editor.find.is_some(), key, *physical_key, *modifiers) {
            publish(shell, Input::Find(input));
            shell.capture_event();
            return;
        }
        if state.focus.is_none() {
            // A find bar field has focus: Tab goes to the next.
            if *key == keyboard::Key::Named(keyboard::key::Named::Tab)
                && !modifiers.command()
                && !modifiers.alt()
            {
                let forward = !modifiers.shift();
                publish(shell, Input::Find(FindInput::Tab { forward }));
                shell.capture_event();
            }
            return;
        }
        // The completion list's keys, while it is up.
        if self.pane == Pane::Text
            && self.editor.choosing()
            && !modifiers.command()
            && !modifiers.alt()
            && let keyboard::Key::Named(named) = key
        {
            use super::complete::CompleteInput;
            let input = match named {
                keyboard::key::Named::ArrowUp => Some(CompleteInput::Move(-1)),
                keyboard::key::Named::ArrowDown => Some(CompleteInput::Move(1)),
                keyboard::key::Named::Enter | keyboard::key::Named::Tab => {
                    Some(CompleteInput::Accept(None))
                }
                keyboard::key::Named::Escape => Some(CompleteInput::Dismiss),
                _ => None,
            };
            if let Some(input) = input {
                publish(shell, Input::Complete(input));
                shell.capture_event();
                return;
            }
        }
        let press = KeyPress {
            key: key.clone(),
            modified_key: modified_key.clone(),
            physical_key: *physical_key,
            modifiers: *modifiers,
            text: text.clone(),
            is_focused: true,
        };
        // Tab types nothing in iced's bindings (its text is a
        // control character); here it indents (REFERENCE-001
        // section 7).
        if *key == keyboard::Key::Named(keyboard::key::Named::Tab) && !modifiers.command() {
            publish(shell, Input::Key(Key::Indent(modifiers.shift())));
            if let Some(focus) = &mut state.focus {
                focus.updated_at = Instant::now();
            }
            shell.capture_event();
            return;
        }
        if let Some(shortcut) = shortcut(key, *physical_key, *modifiers) {
            publish(shell, Input::Key(shortcut));
            if let Some(focus) = &mut state.focus {
                focus.updated_at = Instant::now();
            }
            shell.capture_event();
            return;
        }
        let Some(binding) = Binding::<()>::from_key_press(press) else {
            return;
        };
        // Ctrl/Cmd with a letter is the host's shortcut (Ctrl+S),
        // not typing; AltGr letters come with Alt and still type.
        if matches!(binding, Binding::Insert(_)) && modifiers.command() && !modifiers.alt() {
            return;
        }
        bind(binding, modifiers.shift(), &mut state.pasting, shell);
        if let Some(focus) = &mut state.focus {
            focus.updated_at = Instant::now();
        }
        shell.capture_event();
    }
}

/// Ctrl/Cmd+F and Ctrl/Cmd+H open the find bar (with Shift they are
/// left to the host); while it is open, F3
/// and Ctrl/Cmd+G go to the next match (Shift: the previous one) and
/// Escape closes it.
pub(super) fn find_key(
    open: bool,
    key: &keyboard::Key,
    physical_key: keyboard::key::Physical,
    modifiers: keyboard::Modifiers,
) -> Option<FindInput> {
    let letter = key.to_latin(physical_key);
    let forward = !modifiers.shift();
    match key.as_ref() {
        // With Shift they are the host's (the app's search of every note).
        _ if modifiers.command()
            && !modifiers.alt()
            && !modifiers.shift()
            && matches!(letter, Some('f' | 'h')) =>
        {
            Some(FindInput::Open {
                replace: letter == Some('h'),
            })
        }
        keyboard::Key::Named(keyboard::key::Named::F3) if open => Some(FindInput::Step { forward }),
        _ if open && modifiers.command() && letter == Some('g') => {
            Some(FindInput::Step { forward })
        }
        keyboard::Key::Named(keyboard::key::Named::Escape) if open => Some(FindInput::Close),
        _ => None,
    }
}

/// Carries out a key binding: clipboard ones here, the rest as
/// messages.
pub(super) fn bind(
    binding: Binding<()>,
    shift: bool,
    pasting: &mut Option<clipboard::Kind>,
    shell: &mut Shell<'_, Message>,
) {
    let key = |key| Message(Input::Key(key));
    match binding {
        Binding::Copy => shell.publish(key(Key::Copy(false))),
        Binding::Cut => shell.publish(key(Key::Copy(true))),
        Binding::Paste => {
            *pasting = Some(clipboard::Kind::Text);
            shell.read_clipboard(clipboard::Kind::Text);
        }
        // iced binds only Ctrl+Y to redo; REFERENCE-001 section 15 adds
        // Ctrl+Shift+Z.
        Binding::Undo if shift => shell.publish(key(Key::Redo)),
        Binding::Undo => shell.publish(key(Key::Undo)),
        Binding::Redo => shell.publish(key(Key::Redo)),
        Binding::SelectAll => shell.publish(key(Key::SelectAll)),
        Binding::Insert(c) => shell.publish(key(Key::Insert(c))),
        Binding::Enter if shift => shell.publish(key(Key::SoftEnter)),
        Binding::Enter => shell.publish(key(Key::Enter)),
        Binding::Backspace => shell.publish(key(Key::Delete(Motion::Left))),
        Binding::BackspaceWord => shell.publish(key(Key::Delete(Motion::WordLeft))),
        Binding::BackspaceLine => shell.publish(key(Key::Delete(Motion::LineStart))),
        Binding::Delete => shell.publish(key(Key::Delete(Motion::Right))),
        Binding::DeleteWord => shell.publish(key(Key::Delete(Motion::WordRight))),
        Binding::DeleteLine => shell.publish(key(Key::Delete(Motion::LineEnd))),
        Binding::Unfocus => shell.publish(key(Key::Collapse)),
        Binding::Move(motion) | Binding::Select(motion) => {
            let extend = matches!(binding, Binding::Select(_));
            shell.publish(key(motion_key(motion, extend)));
        }
        Binding::Sequence(bindings) => {
            for binding in bindings {
                bind(binding, shift, pasting, shell);
            }
        }
        Binding::SelectWord | Binding::SelectLine | Binding::Custom(()) => {}
    }
}

fn motion_key(motion: IcedMotion, extend: bool) -> Key {
    let line = |motion| Key::Move(motion, extend);
    match motion {
        IcedMotion::Left => line(Motion::Left),
        IcedMotion::Right => line(Motion::Right),
        IcedMotion::WordLeft => line(Motion::WordLeft),
        IcedMotion::WordRight => line(Motion::WordRight),
        IcedMotion::Home => line(Motion::LineStart),
        IcedMotion::End => line(Motion::LineEnd),
        IcedMotion::DocumentStart => line(Motion::DocumentStart),
        IcedMotion::DocumentEnd => line(Motion::DocumentEnd),
        IcedMotion::Up => Key::Vertical(Vertical::Up, extend),
        IcedMotion::Down => Key::Vertical(Vertical::Down, extend),
        IcedMotion::PageUp => Key::Vertical(Vertical::PageUp, extend),
        IcedMotion::PageDown => Key::Vertical(Vertical::PageDown, extend),
    }
}

#[cfg(test)]
mod tests {
    use iced::keyboard::{Key as IcedKey, Modifiers, key::Named, key::NativeCode, key::Physical};

    use super::{Key, shortcut};
    use crate::edit::Motion;

    #[test]
    fn ctrl_or_cmd_with_home_and_end_go_to_the_document_ends() {
        let physical = Physical::Unidentified(NativeCode::Unidentified);
        let at = |named, modifiers| shortcut(&IcedKey::Named(named), physical, modifiers);
        assert!(matches!(
            at(Named::End, Modifiers::COMMAND),
            Some(Key::Move(Motion::DocumentEnd, false))
        ));
        assert!(matches!(
            at(Named::Home, Modifiers::COMMAND | Modifiers::SHIFT),
            Some(Key::Move(Motion::DocumentStart, true))
        ));
        // Plain Home and End are iced's: the row's edges first.
        assert!(at(Named::End, Modifiers::empty()).is_none());
        if cfg!(target_os = "macos") {
            assert!(matches!(
                at(Named::ArrowDown, Modifiers::LOGO),
                Some(Key::Move(Motion::DocumentEnd, false))
            ));
        }
    }
}
