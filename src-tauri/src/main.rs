#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    #[cfg(target_os = "macos")]
    if blah3_lib::tap_spike_from_env() {
        return;
    }
    blah3_lib::run()
}
