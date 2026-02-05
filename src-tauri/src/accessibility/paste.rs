use std::process::Command;

/// Paste text at the current cursor position.
/// Uses the clipboard + Cmd+V method for reliability.
pub fn paste_text(text: &str) -> Result<(), String> {
    // Set clipboard
    set_clipboard(text).map_err(|e| e.to_string())?;

    // Small delay to ensure clipboard is set
    std::thread::sleep(std::time::Duration::from_millis(50));

    // Simulate Cmd+V
    let script = r#"
        tell application "System Events"
            keystroke "v" using {command down}
        end tell
    "#;

    let output = Command::new("osascript")
        .arg("-e")
        .arg(script)
        .output()
        .map_err(|e| e.to_string())?;

    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).to_string())
    }
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
