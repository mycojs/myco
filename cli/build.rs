use std::{env, fs};
use std::path::{Path, PathBuf};

use deno_core::{Extension, ExtensionFileSource, ExtensionFileSourceCode, ModuleSpecifier};

use util::transpile;
use util::zip::{zip_directory, ZipOptions};

fn main() {
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let runtime_path = out_dir.join("runtime.js");

    println!("cargo:rerun-if-changed=../runtime/src/index.ts");
    let path = Path::new("../runtime/src/index.ts").canonicalize().expect("Failed to canonicalize path");
    let module_specifier = &ModuleSpecifier::from_file_path(path).expect("Failed to create module specifier");
    let transpiled = transpile::parse_and_gen(module_specifier).expect("Failed to transpile");
    fs::write(runtime_path.clone(), transpiled.source).expect("Failed to write transpiled file");

    let myco_extension = Extension::builder("myco")
        .esm(vec![
            ExtensionFileSource {
                specifier: "ext:myco/main",
                code: ExtensionFileSourceCode::LoadedFromFsDuringSnapshot(runtime_path),
            },
        ])
        .build();

    let snapshot_path = out_dir.join("MYCO_SNAPSHOT.bin");
    let init_zip_path = out_dir.join("MYCO_INIT.zip");

    deno_core::snapshot_util::create_snapshot(deno_core::snapshot_util::CreateSnapshotOptions {
        cargo_manifest_dir: env!("CARGO_MANIFEST_DIR"),
        snapshot_path,
        startup_snapshot: None,
        extensions: vec![myco_extension],
        compression_cb: None,
        snapshot_module_load_cb: None,
    });

    zip_directory("../init", init_zip_path.to_str().unwrap(), ZipOptions {
        strip_prefix: Some("../init".to_string()),
        ..ZipOptions::default()
    }).unwrap();
}
