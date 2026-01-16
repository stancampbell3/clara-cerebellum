use std::env;
use std::path::PathBuf;

fn main() {
    // SWI-Prolog installation paths
    // Can be overridden via SWIPL_HOME environment variable
    let swipl_home = env::var("SWIPL_HOME").unwrap_or_else(|_| {
        "/mnt/vastness/home/stanc/Development/swipl/swipl-devel".to_string()
    });

    let swipl_path = PathBuf::from(&swipl_home);
    let lib_dir = swipl_path.join("build/src");
    let include_dir = swipl_path.join("src");
    let home_dir = swipl_path.join("build/home");

    // Verify the library exists
    let lib_path = lib_dir.join("libswipl.so");
    if !lib_path.exists() {
        panic!(
            "SWI-Prolog library not found at {}. \
             Please set SWIPL_HOME to the SWI-Prolog development directory, \
             or build SWI-Prolog first.",
            lib_path.display()
        );
    }

    // Verify the header exists
    let header_path = include_dir.join("SWI-Prolog.h");
    if !header_path.exists() {
        panic!(
            "SWI-Prolog header not found at {}. \
             Please set SWIPL_HOME to the SWI-Prolog development directory.",
            header_path.display()
        );
    }

    // Verify home directory exists (contains ABI, library, boot files)
    let abi_path = home_dir.join("ABI");
    if !abi_path.exists() {
        panic!(
            "SWI-Prolog home not found at {}. \
             Expected ABI file at {}.",
            home_dir.display(),
            abi_path.display()
        );
    }

    // Tell cargo where to find libswipl
    println!("cargo:rustc-link-search=native={}", lib_dir.display());
    println!("cargo:rustc-link-lib=dylib=swipl");

    // Add rpath for runtime library resolution
    // This ensures the library can be found when running the binary
    println!("cargo:rustc-link-arg=-Wl,-rpath,{}", lib_dir.display());

    // Also set rpath to $ORIGIN for relative paths if we ever install
    println!("cargo:rustc-link-arg=-Wl,-rpath,$ORIGIN");

    // Export include path for potential future use (e.g., bindgen)
    println!("cargo:include={}", include_dir.display());

    // Set environment variables for runtime
    // SWIPL_HOME: the root development directory
    // SWI_HOME_DIR: where Prolog finds its library/boot files (build/home)
    println!("cargo:rustc-env=SWIPL_HOME={}", swipl_home);
    println!("cargo:rustc-env=SWI_HOME_DIR={}", home_dir.display());

    // Re-run if environment or build script changes
    println!("cargo:rerun-if-env-changed=SWIPL_HOME");
    println!("cargo:rerun-if-env-changed=SWI_HOME_DIR");
    println!("cargo:rerun-if-changed=build.rs");

    // Also re-run if the library changes
    println!("cargo:rerun-if-changed={}", lib_path.display());

    println!(
        "cargo:warning=Linking against SWI-Prolog at {} (home: {})",
        swipl_home,
        home_dir.display()
    );
}
