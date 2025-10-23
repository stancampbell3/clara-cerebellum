use std::env;
use std::fs;
use std::path::{Path, PathBuf};

fn collect_c_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).expect("reading directory") {
        let entry = entry.expect("reading entry");
        let path = entry.path();
        if path.is_dir() {
            collect_c_files(&path, out);
        } else if path.extension().and_then(|s| s.to_str()) == Some("c") {
            out.push(path);
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
    build.include(core_dir.clone());
    for file in &c_files {
        build.file(file);
        // ensure Cargo rebuilds when any C source changes
        println!("cargo:rerun-if-changed={}", file.display());
    }
    build.compile("clips");

    // Tell cargo to link the static library
    println!("cargo:rustc-link-lib=static=clips");
    println!("cargo:rustc-link-search=native={}", env::var("OUT_DIR").unwrap());

    // Re-run build.rs if build script or top-level clips-src changes
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed={}", clips_src.display());
}
