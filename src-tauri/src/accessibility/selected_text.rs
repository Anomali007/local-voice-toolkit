#![allow(dead_code)]

use std::process::Command;

const TERMINAL_APPS: &[&str] = &["Terminal", "iTerm2", "Alacritty", "kitty", "WezTerm", "Warp"];

/// Terminals interpret Cmd+C differently, so they use the Edit > Copy menu item instead.
fn is_terminal_app(name: &str) -> bool {
    TERMINAL_APPS.contains(&name)
}

/// Escape a string for embedding inside an AppleScript double-quoted literal.
fn applescript_quote(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

/// Pure decision: did the copy attempt put newly selected text on the pasteboard?
/// `count_before`/`count_after` are NSPasteboard change counts around the copy.
/// If the count did not move, nothing was copied and the clipboard text is stale
/// (the user had nothing selected), so we must not return it.
fn copy_result(count_before: Option<i64>, count_after: Option<i64>, clipboard: &str) -> Option<String> {
    match (count_before, count_after) {
        (Some(b), Some(a)) if a != b => {
            let t = clipboard.trim();
            if t.is_empty() {
                None
            } else {
                Some(t.to_string())
            }
        }
        // Unchanged, or we could not read the counter: fail safe, report no selection.
        _ => None,
    }
}

/// Current NSPasteboard change count (increments on every pasteboard write).
fn pasteboard_change_count() -> Option<i64> {
    let out = Command::new("osascript")
        .args([
            "-l",
            "JavaScript",
            "-e",
            "ObjC.import('AppKit'); $.NSPasteboard.generalPasteboard.changeCount",
        ])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    String::from_utf8_lossy(&out.stdout).trim().parse().ok()
}

/// Get the currently selected text from the frontmost application.
/// Sends a copy command and checks the pasteboard change count so that an empty
/// selection returns None instead of a stale clipboard. The user's previous
/// text clipboard is restored afterwards.
pub fn get_selected_text() -> Option<String> {
    let frontapp_script = r#"
        tell application "System Events"
            set frontApp to name of first application process whose frontmost is true
        end tell
        return frontApp
    "#;

    let frontapp = Command::new("osascript")
        .arg("-e")
        .arg(frontapp_script)
        .output()
        .ok()
        .and_then(|o| {
            if o.status.success() {
                Some(String::from_utf8_lossy(&o.stdout).trim().to_string())
            } else {
                None
            }
        })
        .unwrap_or_default();

    let copy_script = if is_terminal_app(&frontapp) {
        format!(
            r#"
            tell application "System Events"
                tell process "{}"
                    try
                        click menu item "Copy" of menu "Edit" of menu bar 1
                    end try
                end tell
            end tell
            delay 0.15
        "#,
            applescript_quote(&frontapp)
        )
    } else {
        r#"
            tell application "System Events"
                keystroke "c" using {command down}
            end tell
            delay 0.1
        "#
        .to_string()
    };

    let old_clipboard = get_clipboard();
    let count_before = pasteboard_change_count();

    let output = Command::new("osascript")
        .arg("-e")
        .arg(&copy_script)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }

    let count_after = pasteboard_change_count();
    let result = copy_result(count_before, count_after, &get_clipboard().unwrap_or_default());

    // Only restore if our copy actually replaced the clipboard.
    if count_before != count_after {
        if let Some(old) = old_clipboard.filter(|o| !o.is_empty()) {
            let _ = set_clipboard(&old);
        }
    }

    result
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unchanged_count_means_nothing_selected() {
        assert_eq!(copy_result(Some(7), Some(7), "stale clipboard text"), None);
    }

    #[test]
    fn changed_count_returns_trimmed_text() {
        assert_eq!(copy_result(Some(7), Some(8), "  hello world \n"), Some("hello world".into()));
    }

    #[test]
    fn changed_but_blank_is_none() {
        assert_eq!(copy_result(Some(1), Some(2), "  \n"), None);
    }

    #[test]
    fn unreadable_counter_fails_safe() {
        assert_eq!(copy_result(None, Some(2), "text"), None);
        assert_eq!(copy_result(Some(1), None, "text"), None);
        assert_eq!(copy_result(None, None, "text"), None);
    }

    #[test]
    fn terminal_detection_and_quoting() {
        assert!(is_terminal_app("iTerm2"));
        assert!(is_terminal_app("Warp"));
        assert!(!is_terminal_app("Safari"));
        assert_eq!(applescript_quote(r#"a"b\c"#), r#"a\"b\\c"#);
    }
}
