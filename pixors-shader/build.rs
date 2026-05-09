use std::path::Path;
use std::process::Command;

fn compile_slang(slangc: &str, src: &Path, include_dirs: &[&Path], dest: &Path) -> bool {
    println!("cargo:rerun-if-changed={}", src.display());

    let mut cmd = Command::new(slangc);
    cmd.env("LD_LIBRARY_PATH", {
        let home = std::env::var("HOME").unwrap_or_default();
        format!("{home}/.local/lib")
    });
    cmd.arg(src);
    for inc in include_dirs {
        cmd.arg("-I").arg(inc);
    }
    cmd.arg("-target")
        .arg("spirv")
        .arg("-fvk-use-entrypoint-name")
        .output()
        .map(|o| {
            let stderr = String::from_utf8_lossy(&o.stderr);
            if o.status.success() {
                if !stderr.is_empty() && stderr.contains("warning") {
                    for line in stderr.lines() {
                        println!("cargo:warning=slangc: {}", line);
                    }
                }
                !o.stdout.is_empty() && std::fs::write(dest, &o.stdout).is_ok()
            } else {
                for line in stderr.lines() {
                    println!("cargo:warning=slangc error: {}", line);
                }
                false
            }
        })
        .unwrap_or(false)
}

fn compile_dir(dir: &Path, include_dirs: &[&Path], kernels_dir: &Path, slangc: &str) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if name != "lib" {
                compile_dir(&path, include_dirs, kernels_dir, slangc);
            }
        } else if path.extension().is_some_and(|e| e == "slang") {
            let stem = path.file_stem().unwrap().to_str().unwrap();
            let dest = kernels_dir.join(format!("{stem}.spv"));

            if !compile_slang(slangc, &path, include_dirs, &dest) {
                panic!("Failed to compile shader: {}", path.display());
            }
        }
    }
}

fn find_slangc() -> Option<String> {
    if Command::new("slangc").arg("--version").output().is_ok() {
        return Some("slangc".to_string());
    }
    let home = std::env::var("HOME").unwrap_or_default();
    let local = format!("{home}/.local/bin/slangc");
    if Command::new(&local).arg("--version").output().is_ok() {
        return Some(local);
    }
    None
}

fn main() {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let shaders_dir = Path::new(&manifest_dir).join("shaders");
    let kernels_dir = Path::new(&manifest_dir).join("kernels");
    let _ = std::fs::create_dir_all(&kernels_dir);

    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed={}", shaders_dir.display());
    println!("cargo:rerun-if-changed={}", kernels_dir.display());

    let include_dirs: &[&Path] = &[&shaders_dir];

    if !shaders_dir.exists() {
        println!("cargo:rustc-env=SHADER_OUT_DIR={}", kernels_dir.display());
        return;
    }

    let Some(slangc) = find_slangc() else {
        // No slangc — fall back to pre-compiled SPV in kernels/ (checked into git).
        // Verify they exist so the build fails loudly if someone forgets to commit them.
        let missing: Vec<_> = std::fs::read_dir(&shaders_dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter(|e| e.path().extension().is_some_and(|x| x == "slang"))
            .filter(|e| {
                let stem = e.path().file_stem().unwrap().to_str().unwrap().to_owned();
                !kernels_dir.join(format!("{stem}.spv")).exists()
            })
            .map(|e| e.path().display().to_string())
            .collect();

        if missing.is_empty() {
            println!("cargo:warning=slangc not found — using pre-built SPIR-V from kernels/");
        } else {
            panic!(
                "slangc not found and pre-built SPIR-V missing for: {}",
                missing.join(", ")
            );
        }
        println!("cargo:rustc-env=SHADER_OUT_DIR={}", kernels_dir.display());
        return;
    };

    compile_dir(&shaders_dir, include_dirs, &kernels_dir, &slangc);
    println!("cargo:rustc-env=SHADER_OUT_DIR={}", kernels_dir.display());
}
