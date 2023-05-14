use std::{fs, io};
use std::fs::File;
use std::path::PathBuf;
use zip::result::ZipResult;
use zip::ZipArchive;

use crate::myco_toml::MycoToml;

static INIT_FILES: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/MYCO_INIT.zip"));

fn unzip_init_files(dir: &PathBuf) -> ZipResult<()> {
    let check_files_reader = io::Cursor::new(INIT_FILES);
    let mut zip_archive = ZipArchive::new(check_files_reader)?;

    // Iterate through the entries in the ZIP archive
    for i in 0..zip_archive.len() {
        let mut entry = zip_archive.by_index(i)?;
        let out_path = dir.join(entry.name());

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

pub fn init(dir: String) {
    let dir = PathBuf::from(dir);
    if dir.exists() {
        eprintln!("error: Directory already exists");
        return;
    }
    fs::create_dir_all(&dir).unwrap();
    unzip_init_files(&dir).expect("Failed to unzip init files");

    let myco_toml_path = dir.join("myco.toml");

    let myco_toml_contents = fs::read_to_string(&myco_toml_path).expect("Failed to read myco.toml");
    let mut myco_toml = MycoToml::from_str(&myco_toml_contents).expect("Failed to parse myco.toml");
    myco_toml.package.name = dir.file_name().unwrap().to_str().unwrap().to_string();
    let myco_toml_contents = myco_toml.to_string();
    fs::write(myco_toml_path, myco_toml_contents).expect("Failed to write myco.toml");

    fs::rename(dir.join("._gitignore"), dir.join(".gitignore")).expect("Failed to rename ._gitignore");

    println!("Initialized Myco project in {}", dir.to_string_lossy());
}
