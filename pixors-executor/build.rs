use std::path::Path;
use std::process::Command;

/// Compile a single .slang file to SPIR-V in `kernels_dir`.
/// Returns true if the SPV was written successfully.
fn compile_slang(slangc: &str, src: &Path, include_dir: &Path, dest: &Path) -> bool {
    println!("cargo:rerun-if-changed={}", src.display());

    Command::new(slangc)
        .env("LD_LIBRARY_PATH", {
            let home = std::env::var("HOME").unwrap_or_default();
            format!("{home}/.local/lib")
        })
        .arg(src)
        .arg("-I").arg(include_dir)
        .arg("-target").arg("spirv")
        .arg("-fvk-use-entrypoint-name")
        .output()
        .map(|o| {
            if o.status.success() {
                let stderr = String::from_utf8_lossy(&o.stderr);
                if !stderr.is_empty() && !stderr.contains("warning") {
                    eprintln!("slangc ({}): {}", src.display(), stderr.trim());
                }
                !o.stdout.is_empty() && std::fs::write(dest, &o.stdout).is_ok()
            } else {
                eprintln!("slangc error ({}): {}",
                    src.display(), String::from_utf8_lossy(&o.stderr).trim());
                false
            }
        })
        .unwrap_or(false)
}

/// Walk `dir` recursively, compiling every .slang file found.
/// Directories named "lib" are skipped entirely — they contain shared modules only.
fn compile_dir(dir: &Path, shaders_root: &Path, kernels_dir: &Path, slangc: &str) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if name != "lib" {
                compile_dir(&path, shaders_root, kernels_dir, slangc);
            }
        } else if path.extension().is_some_and(|e| e == "slang") {
            let stem = path.file_stem().unwrap().to_str().unwrap();
            let dest = kernels_dir.join(format!("{stem}.spv"));

            if compile_slang(slangc, &path, shaders_root, &dest) {
                let rel = path.strip_prefix(shaders_root).unwrap_or(&path);
                println!("cargo:warning=compiled {} → kernels/{stem}.spv", rel.display());
            } else if !dest.exists() {
                eprintln!("WARNING: {stem}.spv not found and slangc failed");
            }
        }
    }
}

fn main() {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let shaders_dir  = Path::new(&manifest_dir).join("shaders");
    let kernels_dir  = Path::new(&manifest_dir).join("kernels");
    let _ = std::fs::create_dir_all(&kernels_dir);

    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed={}", shaders_dir.display());

    if !shaders_dir.exists() {
        println!("cargo:rustc-env=SHADER_OUT_DIR={}", kernels_dir.display());
        return;
    }

    let home   = std::env::var("HOME").unwrap_or_default();
    let slangc = if Command::new("slangc").arg("--version").output().is_ok() {
        "slangc".to_string()
    } else {
        format!("{home}/.local/bin/slangc")
    };

    compile_dir(&shaders_dir, &shaders_dir, &kernels_dir, &slangc);

    println!("cargo:rustc-env=SHADER_OUT_DIR={}", kernels_dir.display());
}
