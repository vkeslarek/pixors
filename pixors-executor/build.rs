use std::path::Path;
use std::process::Command;

fn main() {
    let out_dir = std::env::var("OUT_DIR").unwrap();
    let dest = Path::new(&out_dir).join("shaders");
    let _ = std::fs::create_dir_all(&dest);

    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let shaders_dir = Path::new(&manifest_dir).join("shaders");
    let Ok(entries) = std::fs::read_dir(&shaders_dir) else {
        return;
    };

    let home = std::env::var("HOME").unwrap_or_default();

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_some_and(|e| e == "slang") {
            let stem = path.file_stem().unwrap().to_str().unwrap();
            let dest_path = dest.join(format!("{stem}.spv"));

             let ok = Command::new("slangc")
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
                 println!("cargo:warning=compiled {stem}.slang via slangc");
             } else {
                 let fallback = Path::new(&manifest_dir).join("kernels").join(format!("{stem}.spv"));
                 if fallback.exists() {
                     std::fs::copy(&fallback, &dest_path).unwrap();
                     println!("cargo:warning=copied {stem}.spv (slangc not found)");
                 }
             }

            println!("cargo:rerun-if-changed={}", path.display());
        }
    }

    println!("cargo:rustc-env=SHADER_OUT_DIR={}", dest.display());
}
