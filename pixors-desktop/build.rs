fn main() {
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();

    if target_os == "windows" {
        let out_dir = std::env::var("OUT_DIR").unwrap();
        // OUT_DIR: target/<triple>/<profile>/build/<crate>-<hash>/out
        // Go up 3 levels to reach target/<triple>/<profile>/ (where the .exe lives)
        let profile_dir = std::path::Path::new(&out_dir)
            .parent()
            .unwrap() // <crate>-<hash>/
            .parent()
            .unwrap() // build/
            .parent()
            .unwrap(); // <profile>/

        let build_dir = profile_dir.join("build");
        if let Ok(entries) = std::fs::read_dir(&build_dir) {
            for entry in entries.flatten() {
                if entry
                    .file_name()
                    .to_str()
                    .unwrap_or("")
                    .starts_with("webview2-com-sys")
                {
                    let dll = entry
                        .path()
                        .join("out")
                        .join("x64")
                        .join("WebView2Loader.dll");
                    if dll.exists() {
                        std::fs::copy(&dll, profile_dir.join("WebView2Loader.dll")).ok();
                        break;
                    }
                }
            }
        }
    }
}
