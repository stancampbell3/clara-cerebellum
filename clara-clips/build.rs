use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

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
    build.include(&core_dir);
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

    // Build the CLIPS executable using the makefile
    println!("cargo:rerun-if-changed={}/makefile", core_dir.display());
    println!("cargo:warning=Building CLIPS executable from makefile...");
    
    let status = Command::new("make")
        .arg("release")
        .current_dir(&core_dir)
        .status()
        .expect("Failed to execute make command. Make sure 'make' is installed.");
    
    if !status.success() {
        panic!("Failed to build CLIPS executable with make");
    }

    // Copy the CLIPS binary to a known location in the workspace root
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let workspace_root = PathBuf::from(&manifest_dir)
        .parent()
        .expect("Failed to get workspace root")
        .to_path_buf();
    
    let clips_binaries_dir = workspace_root.join("clips").join("binaries");
    fs::create_dir_all(&clips_binaries_dir).expect("Failed to create clips/binaries directory");
    
    let source_binary = core_dir.join("clips");
    let dest_binary = clips_binaries_dir.join("clips");
    
    fs::copy(&source_binary, &dest_binary)
        .expect("Failed to copy CLIPS binary to clips/binaries/clips");
    
    println!("cargo:warning=CLIPS binary copied to {}", dest_binary.display());

    // Re-run build.rs if build scripts or top-level clips-src changes
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed={}", clips_src.display());
}
