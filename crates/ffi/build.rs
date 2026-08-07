fn main() {
    let crate_dir = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR not set");

    // Emit the C header LabVIEW's Import Shared Library wizard consumes.
    // The header lives in crates/ffi/include/ — never in lv_src/bin/ (that
    // directory is shared objects only; see CLAUDE.md Output Naming).
    match cbindgen::generate(&crate_dir) {
        Ok(bindings) => {
            bindings.write_to_file(format!("{crate_dir}/include/nominal_ffi.h"));
        }
        // A parse error mid-edit shouldn't break `cargo check`/`cargo test`;
        // release builds still surface header drift through CI's build-all.
        Err(err) => println!("cargo:warning=cbindgen failed: {err}"),
    }

    println!("cargo:rerun-if-changed=src");
    println!("cargo:rerun-if-changed=cbindgen.toml");
}
