//! Build script for pico_rust_example
//!
//! This build script handles conditional linking of the defmt linker script
//! based on the enabled features.

fn main() {
    // Only link defmt.x when the defmt feature is enabled (dev builds)
    if std::env::var("CARGO_FEATURE_DEFMT").is_ok() {
        println!("cargo:rustc-link-arg=-Tdefmt.x");
    }

    // Re-run if features change
    println!("cargo:rerun-if-env-changed=CARGO_FEATURE_DEFMT");
}
