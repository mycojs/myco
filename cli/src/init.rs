use log::{debug, info, warn};
use std::fs::File;
use std::path::PathBuf;
use std::{fs, io};
use url::Url;

use zip::result::ZipResult;
use zip::ZipArchive;

use crate::errors::MycoError;
use crate::manifest::{Location, MycoToml};

static INIT_FILES: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/MYCO_INIT.zip"));

fn unzip_init_files(dir: &PathBuf) -> ZipResult<()> {
    debug!("Extracting init files to: {}", dir.display());
    let check_files_reader = io::Cursor::new(INIT_FILES);
    let mut zip_archive = ZipArchive::new(check_files_reader)?;

    debug!("ZIP archive contains {} entries", zip_archive.len());
    // Iterate through the entries in the ZIP archive
    for i in 0..zip_archive.len() {
        let mut entry = zip_archive.by_index(i)?;
        let out_path = dir.join(entry.name());

        if entry.is_dir() {
            // Create a new directory if the entry is a directory
            debug!("Creating directory: {}", out_path.display());
            std::fs::create_dir_all(&out_path)?;
        } else {
            // Create a new file and write the entry's contents to it
            debug!("Extracting file: {}", out_path.display());
            let mut out_file = File::create(&out_path)?;
            io::copy(&mut entry, &mut out_file)?;
        }
    }

    debug!("Successfully extracted all init files");
    Ok(())
}

pub fn init(dir: String) -> Result<(), MycoError> {
    info!("Initializing Myco project in directory: {}", dir);
    let dir = PathBuf::from(&dir);

    if dir.exists() {
        warn!("Directory already exists: {}", dir.display());
        return Err(MycoError::DirectoryExists {
            path: dir.display().to_string(),
        });
    }

    debug!("Creating project directory: {}", dir.display());
    fs::create_dir_all(&dir).map_err(|e| MycoError::DirectoryCreation {
        path: dir.display().to_string(),
        source: e,
    })?;

    info!("Extracting template files");
    unzip_init_files(&dir).map_err(|e| MycoError::InitFileExtraction { source: e })?;

    let myco_toml_path = dir.join("myco.toml");
    debug!("Loading myco.toml from: {}", myco_toml_path.display());

    let (_, mut myco_toml) =
        MycoToml::load_nearest(dir.clone()).map_err(|e| MycoError::InitManifestLoad {
            source: Box::new(e),
        })?;

    // Update package name based on directory name
    myco_toml.package.as_mut().map(|p| {
        if let Some(file_name) = dir.file_name() {
            if let Some(name_str) = file_name.to_str() {
                debug!("Setting package name to: {}", name_str);
                p.name = name_str.to_string();
            }
        }
    });

    // Add default registry
    myco_toml.registries.as_mut().map(|r| {
        if let Ok(url) = Url::parse("https://mycojs.github.io/registry/index.toml") {
            debug!("Adding default registry: {}", url);
            r.insert("myco".to_string(), Location::Url(url));
        }
    });

    debug!("Writing updated myco.toml");
    let myco_toml_contents = myco_toml.to_string()?;
    fs::write(myco_toml_path, myco_toml_contents).map_err(|e| MycoError::FileWrite {
        path: "myco.toml".to_string(),
        source: e,
    })?;

    info!("Project initialization completed successfully");
    Ok(())
}
