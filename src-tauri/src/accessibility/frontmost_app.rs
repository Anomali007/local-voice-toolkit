use serde::{Deserialize, Serialize};
use std::process::Command;

/// Information about the frontmost (active) application
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrontmostAppInfo {
    pub name: String,
    pub bundle_id: String,
}

/// Get information about the frontmost application using AppleScript
pub fn get_frontmost_app() -> Option<FrontmostAppInfo> {
    // AppleScript to get frontmost app name and bundle ID
    let script = r#"
        tell application "System Events"
            set frontApp to first application process whose frontmost is true
            set appName to name of frontApp
            set bundleID to bundle identifier of frontApp
            return appName & "|" & bundleID
        end tell
    "#;

    let output = Command::new("osascript")
        .arg("-e")
        .arg(script)
        .output()
        .ok()?;

    if !output.status.success() {
        tracing::warn!(
            "Failed to get frontmost app: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        return None;
    }

    let result = String::from_utf8_lossy(&output.stdout);
    let result = result.trim();

    // Parse "AppName|com.app.bundleid"
    let parts: Vec<&str> = result.splitn(2, '|').collect();
    if parts.len() == 2 {
        Some(FrontmostAppInfo {
            name: parts[0].to_string(),
            bundle_id: parts[1].to_string(),
        })
    } else {
        // Fallback: just use the whole output as name
        Some(FrontmostAppInfo {
            name: result.to_string(),
            bundle_id: String::new(),
        })
    }
}
