use std::fs::File;
use std::io;
use zip::result::ZipResult;
use zip::ZipArchive;
use crate::myco_toml::MycoToml;

static CHECK_FILES: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/MYCO_CHECK.zip"));

fn unzip_check_files() -> ZipResult<()> {
    let check_files_reader = io::Cursor::new(CHECK_FILES);
    let mut zip_archive = ZipArchive::new(check_files_reader)?;

    // Iterate through the entries in the ZIP archive
    for i in 0..zip_archive.len() {
        let mut entry = zip_archive.by_index(i)?;
        let out_path = format!("{}{}", "./", entry.name());

        if entry.is_dir() {
            // Create a new directory if the entry is a directory
            std::fs::create_dir_all(&out_path)?;
        } else {
            // Create a new file and write the entry's contents to it
            let mut out_file = File::create(&out_path)?;
            io::copy(&mut entry, &mut out_file)?;
        }
    }

    Ok(())
}

pub fn check(myco_toml: MycoToml) {
    unzip_check_files().expect("Should have worked");
    crate::run::run_file("vendor/myco_check");
}
