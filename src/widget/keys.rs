//! Shortcuts the surface handles before iced's text bindings: formatting
//! (REFERENCE-001 section 12), source mode (section 17), and the line
//! commands of CodeMirror's default keymap (`edit::lines`). Letters are
//! read from the key whatever the layout; with Alt they are left alone, so
//! AltGr letters still type.
use iced::keyboard::{self, Modifiers, key::Named};

use super::Key;
use crate::edit::format::Format;

/// The shortcut `key` with `modifiers` is, if any.
pub(super) fn shortcut(
    key: &keyboard::Key,
    physical_key: keyboard::key::Physical,
    modifiers: Modifiers,
) -> Option<Key> {
    let letter = key.to_latin(physical_key);
    let command = modifiers.command() && !modifiers.alt();
    match key.as_ref() {
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
            _ => None,
        },
        _ => None,
    }
}
