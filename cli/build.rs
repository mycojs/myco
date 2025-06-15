use std::path::{Path, PathBuf};
use std::{env, fs};

use util::transpile;
use util::zip::{zip_directory, ZipOptions};

fn main() {
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let runtime_path = out_dir.join("runtime.js");

    println!("cargo:rerun-if-changed=../runtime/src/index.ts");
    println!("cargo:rerun-if-changed=../init");
    let path = Path::new("../runtime/src/index.ts")
        .canonicalize()
        .expect("Failed to canonicalize path");

    // Transpile the runtime TypeScript to JavaScript
    let transpiled = transpile::parse_and_gen_path(&path).expect("Failed to transpile");
    fs::write(runtime_path.clone(), transpiled.source).expect("Failed to write transpiled file");

    // Create a dummy snapshot file for now - we'll create the actual snapshot at runtime
    let snapshot_path = out_dir.join("MYCO_SNAPSHOT.bin");
    fs::write(snapshot_path, b"").expect("Failed to write dummy snapshot");

    let init_zip_path = out_dir.join("MYCO_INIT.zip");
    zip_directory(
        "../init",
        init_zip_path.to_str().unwrap(),
        ZipOptions {
            strip_prefix: Some("../init".to_string()),
            ..ZipOptions::default()
        },
    )
    .unwrap();
}
