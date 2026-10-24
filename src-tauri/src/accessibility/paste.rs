#![allow(dead_code)]

use std::process::Command;

/// Paste text at the current cursor position.
/// Uses the clipboard + Cmd+V method for reliability.
///
/// On success, any previous *text* clipboard contents are restored shortly
/// after the paste so dictation doesn't clobber the user's clipboard.
/// On failure, the transcription is intentionally left in the clipboard as a
/// manual-paste fallback (the caller should tell the user to press Cmd+V).
pub fn paste_text(text: &str) -> Result<(), String> {
    // Remember what was in the clipboard before we overwrite it
    let previous = get_clipboard_text();

    // Set clipboard
    set_clipboard(text).map_err(|e| e.to_string())?;

    // Small delay to ensure clipboard is set
    std::thread::sleep(std::time::Duration::from_millis(50));

    // Post Cmd+V straight to the system. The old route (osascript telling
    // System Events to keystroke) also needed an Automation grant and failed
    // silently without it.
    let pasted = send_cmd_v();

    if pasted.is_ok() {
        // Restore the previous clipboard after the target app has consumed
        // the paste. Only text is preserved (pbpaste limitation).
        if let Some(prev) = previous.filter(|p| !p.is_empty()) {
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(400));
                if let Err(e) = set_clipboard(&prev) {
                    tracing::warn!("Failed to restore previous clipboard: {}", e);
                }
            });
        }
        Ok(())
    } else {
        pasted
    }
}

/// Press and release Cmd+V in the frontmost app.
fn send_cmd_v() -> Result<(), String> {
    use core_graphics::event::{CGEvent, CGEventFlags, CGEventTapLocation};
    use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};

    // Posting keyboard events is silently dropped without Accessibility.
    if !crate::commands::permissions::accessibility_trusted() {
        return Err("Accessibility permission is not granted".into());
    }
    const KEY_V: u16 = 9; // kVK_ANSI_V
    let source = CGEventSource::new(CGEventSourceStateID::CombinedSessionState)
        .map_err(|_| "Could not create keyboard event source".to_string())?;
    for down in [true, false] {
        let event = CGEvent::new_keyboard_event(source.clone(), KEY_V, down)
            .map_err(|_| "Could not create Cmd+V event".to_string())?;
        // Only Command: a still-held Option must not turn this into Option+Cmd+V.
        event.set_flags(CGEventFlags::CGEventFlagCommand);
        event.post(CGEventTapLocation::HID);
    }
    Ok(())
}

/// Read the current text clipboard contents, if any.
fn get_clipboard_text() -> Option<String> {
    Command::new("pbpaste")
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
}

/// Put `text` on the clipboard without pasting.
pub fn copy_to_clipboard(text: &str) -> Result<(), std::io::Error> {
    set_clipboard(text)
}

/// Set the clipboard contents
fn set_clipboard(text: &str) -> Result<(), std::io::Error> {
    use std::io::Write;
    use std::process::Stdio;

    let mut child = Command::new("pbcopy")
        .stdin(Stdio::piped())
        .spawn()?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(text.as_bytes())?;
    }

    child.wait()?;
    Ok(())
}

/// Type text character by character using CGEvents
/// This is slower but doesn't touch the clipboard
#[cfg(target_os = "macos")]
pub fn type_text(_text: &str) -> Result<(), String> {
    // This would use CGEventCreateKeyboardEvent and CGEventPost
    // For now, we use the clipboard method which is faster for longer text
    //
    // Implementation would look like:
    // for char in text.chars() {
    //     let event = CGEventCreateKeyboardEvent(source, keycode, true);
    //     CGEventKeyboardSetUnicodeString(event, char);
    //     CGEventPost(kCGHIDEventTap, event);
    // }

    Err("Not implemented - use paste_text instead".to_string())
}
