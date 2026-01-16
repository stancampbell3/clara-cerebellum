use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());

    let swipl_src = manifest_dir.join("swipl-src");
    let swipl_build = out_dir.join("swipl-build");

    // Verify source exists
    if !swipl_src.join("CMakeLists.txt").exists() {
        panic!(
            "SWI-Prolog source not found at {}. \
             Please ensure swipl-src directory is populated.",
            swipl_src.display()
        );
    }

    // Create build directory
    fs::create_dir_all(&swipl_build).expect("Failed to create build directory");

    // Detect available generator (prefer Ninja, fall back to Make)
    let generator = if Command::new("ninja").arg("--version").output().is_ok() {
        "Ninja"
    } else {
        "Unix Makefiles"
    };

    // Run CMake configure
    let mut cmake_args = vec![
        "-G".to_string(),
        generator.to_string(),
        "-S".to_string(),
        swipl_src.to_str().unwrap().to_string(),
        "-B".to_string(),
        swipl_build.to_str().unwrap().to_string(),
        "-DCMAKE_BUILD_TYPE=Release".to_string(),
        "-DMULTI_THREADED=ON".to_string(),
        "-DSWIPL_PACKAGES=OFF".to_string(),
        "-DINSTALL_DOCUMENTATION=OFF".to_string(),
    ];

    // macOS-specific: disable tcmalloc
    if cfg!(target_os = "macos") {
        cmake_args.push("-DUSE_TCMALLOC=OFF".to_string());
    }

    let status = Command::new("cmake")
        .args(&cmake_args)
        .status()
        .expect("Failed to run cmake. Is cmake installed?");

    if !status.success() {
        panic!("CMake configure failed");
    }

    // Run CMake build (parallel)
    let status = Command::new("cmake")
        .args(["--build", swipl_build.to_str().unwrap(), "--parallel"])
        .status()
        .expect("Failed to build SWI-Prolog");

    if !status.success() {
        panic!("CMake build failed");
    }

    // Determine library name by platform
    let lib_name = if cfg!(target_os = "macos") {
        "libswipl.dylib"
    } else {
        "libswipl.so"
    };

    let lib_dir = swipl_build.join("src");
    let home_dir = swipl_build.join("home");

    // Verify build outputs
    if !lib_dir.join(lib_name).exists() {
        panic!("Build succeeded but {} not found", lib_name);
    }

    // Link configuration
    println!("cargo:rustc-link-search=native={}", lib_dir.display());
    println!("cargo:rustc-link-lib=dylib=swipl");

    // Platform-specific rpath for runtime library resolution
    if cfg!(target_os = "macos") {
        println!("cargo:rustc-link-arg=-Wl,-rpath,{}", lib_dir.display());
    } else {
        println!("cargo:rustc-link-arg=-Wl,-rpath,{}", lib_dir.display());
        println!("cargo:rustc-link-arg=-Wl,-rpath,$ORIGIN");
    }

    // Export paths for runtime
    println!("cargo:rustc-env=SWI_HOME_DIR={}", home_dir.display());
    println!("cargo:include={}", swipl_src.join("src").display());

    // Rebuild triggers
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=swipl-src/CMakeLists.txt");
    println!("cargo:rerun-if-changed=swipl-src/src");

    println!(
        "cargo:warning=SWI-Prolog built at {} (home: {})",
        lib_dir.display(),
        home_dir.display()
    );
}
