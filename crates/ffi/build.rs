fn main() {
    let crate_dir = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR not set");

    // Emit the C header LabVIEW's Import Shared Library wizard consumes.
    // Canonical copy in crates/ffi/include/, plus a convenience copy next to
    // the shared objects in lv_src/bin/ so everything LabVIEW needs sits in
    // one folder.
    match cbindgen::generate(&crate_dir) {
        Ok(bindings) => {
            let header = format!("{crate_dir}/include/nominal_ffi.h");
            bindings.write_to_file(&header);

            let bin_dir = format!("{crate_dir}/../../lv_src/bin");
            if let Err(err) = std::fs::create_dir_all(&bin_dir)
                .and_then(|()| std::fs::copy(&header, format!("{bin_dir}/nominal_ffi.h")))
            {
                println!("cargo:warning=failed to copy header into lv_src/bin: {err}");
            }
        }
        // A parse error mid-edit shouldn't break `cargo check`/`cargo test`;
        // release builds still surface header drift through CI's build-all.
        Err(err) => println!("cargo:warning=cbindgen failed: {err}"),
    }

    println!("cargo:rerun-if-changed=src");
    println!("cargo:rerun-if-changed=cbindgen.toml");
}
