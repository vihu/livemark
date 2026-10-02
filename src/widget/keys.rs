//! Shortcuts the surface handles before iced's text bindings: formatting
//! (REFERENCE-001 section 12), source mode (section 17), the document's
//! start and end (section 13), and the line commands of CodeMirror's
//! default keymap (`edit::lines`). Letters are
//! read from the key whatever the layout; with Alt they are left alone, so
//! AltGr letters still type.
use iced::keyboard::{self, Modifiers, key::Named};

use super::Key;
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
            _ => None,
        },
        _ => None,
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
