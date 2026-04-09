use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct PermissionStatus {
    pub microphone: bool,
    pub accessibility: bool,
}

/// Check accessibility permission using AXIsProcessTrusted() from ApplicationServices framework
fn check_accessibility() -> bool {
    #[link(name = "ApplicationServices", kind = "framework")]
    extern "C" {
        fn AXIsProcessTrusted() -> bool;
    }
    unsafe { AXIsProcessTrusted() }
}

/// Check microphone permission by verifying a default input device is available
fn check_microphone() -> bool {
    use cpal::traits::HostTrait;
    let host = cpal::default_host();
    host.default_input_device().is_some()
}

#[tauri::command]
pub fn check_permissions() -> PermissionStatus {
    PermissionStatus {
        microphone: check_microphone(),
        accessibility: check_accessibility(),
    }
}

#[tauri::command]
pub fn open_system_settings(pane: String) -> Result<(), String> {
    let url = match pane.as_str() {
        "microphone" => "x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone",
        "accessibility" => "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility",
        _ => return Err(format!("Unknown settings pane: {}", pane)),
    };

    // Try URL scheme first, fall back to AppleScript
    let result = std::process::Command::new("open").arg(url).status();
    match result {
        Ok(status) if status.success() => Ok(()),
        _ => {
            // Fallback: use AppleScript to open System Settings directly
            let script = match pane.as_str() {
                "microphone" => r#"tell application "System Settings" to activate
                    delay 0.5
                    tell application "System Events"
                        tell process "System Settings"
                            click menu item "Privacy & Security" of menu "View" of menu bar 1
                        end tell
                    end tell"#,
                "accessibility" => r#"tell application "System Settings" to activate
                    delay 0.5
                    tell application "System Events"
                        tell process "System Settings"
                            click menu item "Privacy & Security" of menu "View" of menu bar 1
                        end tell
                    end tell"#,
                _ => return Err("Unknown pane".to_string()),
            };
            std::process::Command::new("osascript")
                .arg("-e")
                .arg(script)
                .status()
                .map_err(|e| format!("Failed to open System Settings: {}", e))?;
            Ok(())
        }
    }
}
