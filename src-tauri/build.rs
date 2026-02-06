fn main() {
    // Link ApplicationServices framework for AXIsProcessTrusted
    println!("cargo:rustc-link-lib=framework=ApplicationServices");
    tauri_build::build()
}
