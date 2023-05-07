use deno_core::include_js_files;
use deno_core::Extension;
use std::env;
use std::path::PathBuf;

fn main() {
    let myco_extension = Extension::builder("myco")
        .esm(include_js_files!("src/myco.js",))
        .build();

    let out_dir = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let snapshot_path = out_dir.join("MYCO_SNAPSHOT.bin");

    deno_core::snapshot_util::create_snapshot(deno_core::snapshot_util::CreateSnapshotOptions {
        cargo_manifest_dir: env!("CARGO_MANIFEST_DIR"),
        snapshot_path,
        startup_snapshot: None,
        extensions: vec![myco_extension],
        compression_cb: None,
        snapshot_module_load_cb: None,
    })
}
