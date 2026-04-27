fn main() {
    #[cfg(target_os = "windows")]
    {
        // Copy WebView2Loader.dll next to the binary
        let out = std::env::var("OUT_DIR").unwrap();
        // OUT_DIR is something like target/release/build/webview2-com-sys-HASH/out
        // Go up from OUT_DIR to find the x64 dll
        let build_dir = std::path::Path::new(&out)
            .parent().unwrap()  // out/
            .parent().unwrap(); // webview2-com-sys-HASH/

        // Find the x64 DLL in any webview2-com-sys build directory
        let target_profile = std::env::var("PROFILE").unwrap();
        let search_base = build_dir.parent().unwrap(); // build/
        for entry in std::fs::read_dir(search_base).unwrap() {
            let entry = entry.unwrap();
            if entry.file_name().to_str().unwrap().starts_with("webview2-com-sys") {
                let dll = entry.path().join("out").join("x64").join("WebView2Loader.dll");
                if dll.exists() {
                    let dest = std::path::Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap())
                        .parent().unwrap()
                        .join("target")
                        .join(&target_profile)
                        .join("WebView2Loader.dll");
                    std::fs::copy(&dll, &dest).ok();
                    println!("cargo:warning=Copied WebView2Loader.dll to {}", dest.display());
                    break;
                }
            }
        }
    }
}
