use std::env;
use std::fs::File;
use std::io::{Seek, Write};
use std::io::prelude::*;
use std::iter::Iterator;
use std::path::{Path, PathBuf};

use deno_core::Extension;
use deno_core::include_js_files;
use zip::result::{ZipError, ZipResult};
use zip::write::FileOptions;

use walkdir::{DirEntry, WalkDir};

fn main() {
    let myco_extension = Extension::builder("myco")
        .esm(include_js_files!(runtime "runtime/dist/index.js",))
        .build();

    let out_dir = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let snapshot_path = out_dir.join("MYCO_SNAPSHOT.bin");
    let check_zip_path = out_dir.join("MYCO_CHECK.zip");

    deno_core::snapshot_util::create_snapshot(deno_core::snapshot_util::CreateSnapshotOptions {
        cargo_manifest_dir: env!("CARGO_MANIFEST_DIR"),
        snapshot_path,
        startup_snapshot: None,
        extensions: vec![myco_extension],
        compression_cb: None,
        snapshot_module_load_cb: None,
    });

    zip_directory("check", check_zip_path.to_str().unwrap(), zip::CompressionMethod::Deflated).unwrap();
}

fn zip_dir<T>(
    it: &mut dyn Iterator<Item = DirEntry>,
    prefix: &str,
    writer: T,
    method: zip::CompressionMethod,
) -> zip::result::ZipResult<()>
    where
        T: Write + Seek,
{
    let mut zip = zip::ZipWriter::new(writer);
    let options = FileOptions::default()
        .compression_method(method)
        .unix_permissions(0o755);

    let mut buffer = Vec::new();
    for entry in it {
        let path = entry.path();
        let name = path.strip_prefix(Path::new(prefix)).unwrap();

        // Write file or directory explicitly
        // Some unzip tools unzip files with directory paths correctly, some do not!
        if path.is_file() {
            println!("adding file {path:?} as {name:?} ...");
            #[allow(deprecated)]
            zip.start_file_from_path(name, options)?;
            let mut f = File::open(path)?;

            f.read_to_end(&mut buffer)?;
            zip.write_all(&buffer)?;
            buffer.clear();
        } else if !name.as_os_str().is_empty() {
            // Only if not root! Avoids path spec / warning
            // and mapname conversion failed error on unzip
            println!("adding dir {path:?} as {name:?} ...");
            #[allow(deprecated)]
            zip.add_directory_from_path(name, options)?;
        }
    }
    zip.finish()?;
    Result::Ok(())
}

fn zip_directory(
    src_dir: &str,
    dst_file: &str,
    method: zip::CompressionMethod,
) -> ZipResult<()> {
    if !Path::new(src_dir).is_dir() {
        return Err(ZipError::FileNotFound);
    }

    let path = Path::new(dst_file);
    let file = File::create(path).unwrap();

    let walkdir = WalkDir::new(src_dir);
    let it = walkdir.into_iter();

    zip_dir(&mut it.filter_map(|e| e.ok()), src_dir, file, method)?;

    Ok(())
}