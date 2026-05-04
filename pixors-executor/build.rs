use std::path::Path;
use std::process::Command;

fn main() {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let shaders_dir = Path::new(&manifest_dir).join("shaders");
    let kernels_dir = Path::new(&manifest_dir).join("kernels");
    let _ = std::fs::create_dir_all(&kernels_dir);

    // Always rerun when shaders or this script change.
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed={}", shaders_dir.display());

    let home = std::env::var("HOME").unwrap_or_default();

    let Ok(entries) = std::fs::read_dir(&shaders_dir) else {
        // Point to kernels/ even if shaders/ doesn't exist (pre-compiled fallback).
        println!("cargo:rustc-env=SHADER_OUT_DIR={}", kernels_dir.display());
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_some_and(|e| e == "slang") {
            let stem = path.file_stem().unwrap().to_str().unwrap();
            // Compile directly to the stable kernels/ directory.
            let dest_path = kernels_dir.join(format!("{stem}.spv"));

            // Try slangc from PATH, then fall back to ~/.local/bin/slangc.
            let slangc = if Command::new("slangc").arg("--version").output().is_ok() {
                "slangc".to_string()
            } else {
                format!("{home}/.local/bin/slangc")
            };

            let ok = Command::new(&slangc)
                .env("LD_LIBRARY_PATH", format!("{home}/.local/lib"))
                .arg(&path)
                .arg("-I")
                .arg(shaders_dir.to_str().unwrap())
                .arg("-target")
                .arg("spirv")
                .arg("-fvk-use-entrypoint-name")
                .output()
                .map(|o| {
                    if o.status.success() {
                        let stderr = String::from_utf8_lossy(&o.stderr);
                        if !stderr.is_empty() && !stderr.contains("warning") {
                            eprintln!("slangc: {}", stderr.trim());
                        }
                        !o.stdout.is_empty() && std::fs::write(&dest_path, &o.stdout).is_ok()
                    } else {
                        let stderr = String::from_utf8_lossy(&o.stderr);
                        eprintln!("slangc error: {}", stderr.trim());
                        false
                    }
                })
                .unwrap_or(false);

            if ok {
                println!("cargo:warning=compiled {stem}.slang → kernels/{stem}.spv");
            } else if !dest_path.exists() {
                eprintln!("WARNING: {stem}.spv not found and slangc failed");
            }

            println!("cargo:rerun-if-changed={}", path.display());
        }
    }

    // Deterministic path — never invalidated by hash changes.
    println!("cargo:rustc-env=SHADER_OUT_DIR={}", kernels_dir.display());
}
