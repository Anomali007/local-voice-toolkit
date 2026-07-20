use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct PermissionStatus {
    pub microphone: bool,
    pub accessibility: bool,
    /// Fine-grained TCC state: "authorized" | "denied" | "restricted" | "not_determined" | "unknown"
    pub microphone_status: String,
}

/// Microphone TCC authorization state (AVAuthorizationStatus).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MicAuthStatus {
    NotDetermined,
    Restricted,
    Denied,
    Authorized,
    Unknown,
}

impl MicAuthStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            MicAuthStatus::NotDetermined => "not_determined",
            MicAuthStatus::Restricted => "restricted",
            MicAuthStatus::Denied => "denied",
            MicAuthStatus::Authorized => "authorized",
            MicAuthStatus::Unknown => "unknown",
        }
    }
}

/// AVFoundation FFI: query/request microphone access so the system prompt
/// actually appears (CoreAudio alone doesn't reliably surface it for us).
mod av {
    use objc2::msg_send;
    use objc2::runtime::{AnyObject, Bool};

    #[link(name = "AVFoundation", kind = "framework")]
    extern "C" {
        /// NSString* AVMediaTypeAudio
        static AVMediaTypeAudio: *const AnyObject;
    }

    pub fn authorization_status() -> i64 {
        unsafe {
            let cls = objc2::class!(AVCaptureDevice);
            let media: *const AnyObject = AVMediaTypeAudio;
            msg_send![cls, authorizationStatusForMediaType: media]
        }
    }

    /// Trigger the system microphone prompt. `callback` fires with the user's
    /// answer (also fires immediately if already determined).
    pub fn request_access(callback: impl Fn(bool) + Send + 'static) {
        unsafe {
            let block = block2::RcBlock::new(move |granted: Bool| {
                callback(granted.as_bool());
            });
            let cls = objc2::class!(AVCaptureDevice);
            let media: *const AnyObject = AVMediaTypeAudio;
            let _: () = msg_send![cls, requestAccessForMediaType: media, completionHandler: &*block];
        }
    }
}

/// Current microphone TCC status.
pub fn microphone_status() -> MicAuthStatus {
    match av::authorization_status() {
        0 => MicAuthStatus::NotDetermined,
        1 => MicAuthStatus::Restricted,
        2 => MicAuthStatus::Denied,
        3 => MicAuthStatus::Authorized,
        _ => MicAuthStatus::Unknown,
    }
}

/// Fire the system microphone prompt without waiting for the answer.
pub fn prompt_microphone_access() {
    av::request_access(|granted| {
        tracing::info!("Microphone access prompt answered: granted={}", granted);
    });
}

/// Check accessibility permission using AXIsProcessTrusted() from ApplicationServices framework
fn check_accessibility() -> bool {
    #[link(name = "ApplicationServices", kind = "framework")]
    extern "C" {
        fn AXIsProcessTrusted() -> bool;
    }
    unsafe { AXIsProcessTrusted() }
}

/// Whether the process is trusted for Accessibility.
pub fn accessibility_trusted() -> bool {
    check_accessibility()
}

/// Show the system Accessibility-permission dialog (the one that offers to
/// open System Settings). Returns the current trust state.
pub fn prompt_accessibility() -> bool {
    use core_foundation::base::TCFType;
    use core_foundation::boolean::CFBoolean;
    use core_foundation::dictionary::{CFDictionary, CFDictionaryRef};
    use core_foundation::string::{CFString, CFStringRef};

    #[link(name = "ApplicationServices", kind = "framework")]
    extern "C" {
        fn AXIsProcessTrustedWithOptions(options: CFDictionaryRef) -> bool;
        static kAXTrustedCheckOptionPrompt: CFStringRef;
    }

    unsafe {
        let key = CFString::wrap_under_get_rule(kAXTrustedCheckOptionPrompt);
        let dict = CFDictionary::from_CFType_pairs(&[(
            key.as_CFType(),
            CFBoolean::true_value().as_CFType(),
        )]);
        AXIsProcessTrustedWithOptions(dict.as_concrete_TypeRef())
    }
}

/// Show the Accessibility prompt at most once per app run (used by paths that
/// retry, like the right-Option event tap).
pub fn prompt_accessibility_once() {
    use std::sync::atomic::{AtomicBool, Ordering};
    static PROMPTED: AtomicBool = AtomicBool::new(false);
    if !PROMPTED.swap(true, Ordering::SeqCst) {
        tracing::info!("Prompting for Accessibility permission");
        prompt_accessibility();
    }
}

#[tauri::command]
pub fn check_permissions() -> PermissionStatus {
    let mic = microphone_status();
    PermissionStatus {
        microphone: mic == MicAuthStatus::Authorized,
        accessibility: check_accessibility(),
        microphone_status: mic.as_str().to_string(),
    }
}

/// Trigger the real system microphone prompt (when not yet determined) and
/// resolve with the final status string.
#[tauri::command]
pub async fn request_microphone_access() -> String {
    let status = microphone_status();
    if status != MicAuthStatus::NotDetermined {
        return status.as_str().to_string();
    }

    let (tx, rx) = tokio::sync::oneshot::channel::<bool>();
    let tx = std::sync::Mutex::new(Some(tx));
    av::request_access(move |granted| {
        if let Ok(mut guard) = tx.lock() {
            if let Some(tx) = guard.take() {
                let _ = tx.send(granted);
            }
        }
    });

    match rx.await {
        Ok(true) => "authorized".to_string(),
        Ok(false) => "denied".to_string(),
        Err(_) => "unknown".to_string(),
    }
}

/// Show the system Accessibility dialog. Returns whether the app is trusted
/// (usually false until the user flips the toggle in System Settings).
#[tauri::command]
pub fn request_accessibility_access() -> bool {
    prompt_accessibility()
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
