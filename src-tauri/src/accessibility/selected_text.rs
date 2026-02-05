use std::process::Command;

/// Get the currently selected text from the frontmost application.
/// Uses AppleScript as a reliable cross-app method.
pub fn get_selected_text() -> Option<String> {
    // First, try to get selected text via AppleScript
    // This works for most standard macOS apps
    let script = r#"
        tell application "System Events"
            set frontApp to name of first application process whose frontmost is true
        end tell

        tell application "System Events"
            keystroke "c" using {command down}
        end tell

        delay 0.1

        the clipboard
    "#;

    // Save current clipboard
    let old_clipboard = get_clipboard();

    // Run the AppleScript to copy selection
    let output = Command::new("osascript")
        .arg("-e")
        .arg(script)
        .output()
        .ok()?;

    if output.status.success() {
        let text = String::from_utf8_lossy(&output.stdout).trim().to_string();

        // Restore old clipboard after a short delay
        if let Some(old) = old_clipboard {
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(500));
                let _ = set_clipboard(&old);
            });
        }

        if !text.is_empty() {
            return Some(text);
        }
    }

    None
}

/// Get the current clipboard contents
fn get_clipboard() -> Option<String> {
    let output = Command::new("pbpaste").output().ok()?;
    if output.status.success() {
        Some(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        None
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

/// Alternative: Get selected text using Accessibility API directly
/// This requires accessibility permissions but is more reliable
#[cfg(target_os = "macos")]
pub fn get_selected_text_ax() -> Option<String> {
    // This would use the AXUIElement API directly
    // For now, we use the AppleScript method which is more compatible
    //
    // To implement properly, we'd need:
    // - AXUIElementCreateSystemWide()
    // - AXUIElementCopyAttributeValue for kAXFocusedUIElementAttribute
    // - AXUIElementCopyAttributeValue for kAXSelectedTextAttribute
    //
    // The macos-accessibility-client crate can help with this

    get_selected_text()
}
