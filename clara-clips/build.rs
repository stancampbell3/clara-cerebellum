use std::env;
use std::fs;
use std::path::{Path, PathBuf};

fn collect_c_files(dir: &Path, out: &mut Vec<PathBuf>) {
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                collect_c_files(&path, out);
            } else if path.extension().and_then(|s| s.to_str()) == Some("c") {
                out.push(path);
            }
        }
    }
}

fn main() {
    // Path to CLIPS source
    let clips_src = PathBuf::from("clips-src");
    let core_dir = clips_src.join("core");

    // Collect all .c files under clips-src/core
    let mut c_files = Vec::new();
    collect_c_files(&core_dir, &mut c_files);

    // Compile CLIPS C source into a static library
    let mut build = cc::Build::new();
    build.include(&core_dir)
        .flag("-Wno-unused-parameter")
        .flag("-Wno-cast-function-type");
    for file in &c_files {
        build.file(file);
        // ensure Cargo rebuilds when any C source changes
        println!("cargo:rerun-if-changed={}", file.display());
    }
    build.compile("clips");

    // Tell cargo to link the static library
    println!("cargo:rustc-link-lib=static=clips");
    let out_dir = env::var("OUT_DIR").unwrap();
    println!("cargo:rustc-link-search=native={}", out_dir);

    // Note: We no longer build the standalone CLIPS executable since we're using FFI
    // The standalone executable would require linking Rust functions which complicates the build
    // All CLIPS interaction now goes through the Rust FFI layer

    // Re-run build.rs if build scripts or top-level clips-src changes
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed={}", clips_src.display());
}
