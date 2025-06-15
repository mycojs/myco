use std::fs::File;
use std::io::{Read, Seek, Write};
use std::path::{Path, PathBuf};

use crate::UtilError;
use walkdir::{DirEntry, WalkDir};
use zip::write::FileOptions;
pub use zip::CompressionMethod;

pub struct ZipOptions {
    pub compression_method: CompressionMethod,
    pub strip_prefix: Option<String>,
    pub apply_prefix: Option<String>,
}

impl Default for ZipOptions {
    fn default() -> Self {
        Self {
            compression_method: CompressionMethod::Deflated,
            strip_prefix: None,
            apply_prefix: None,
        }
    }
}

fn zip_dir<T>(
    it: &mut dyn Iterator<Item = DirEntry>,
    writer: T,
    zip_options: ZipOptions,
) -> Result<(), UtilError>
where
    T: Write + Seek,
{
    let mut zip = zip::ZipWriter::new(writer);
    let options = FileOptions::default()
        .compression_method(zip_options.compression_method)
        .unix_permissions(0o755);

    let mut buffer = Vec::new();
    for entry in it {
        let path = entry.path();
        let mut name: PathBuf = path.to_path_buf();
        if let Some(prefix) = zip_options.strip_prefix.as_ref() {
            name = name
                .strip_prefix(prefix)
                .map_err(|e| UtilError::StripPrefix {
                    prefix: prefix.clone(),
                    message: e.to_string(),
                })?
                .to_path_buf();
        }
        if let Some(prefix) = zip_options.apply_prefix.as_ref() {
            name = PathBuf::from(prefix).join(name)
        }
        let name = name.as_path();

        // Write file or directory explicitly
        // Some unzip tools unzip files with directory paths correctly, some do not!
        if path.is_file() {
            #[allow(deprecated)]
            zip.start_file_from_path(name, options)?;
            let mut f = File::open(path)?;

            f.read_to_end(&mut buffer)?;
            zip.write_all(&buffer)?;
            buffer.clear();
        } else if !name.as_os_str().is_empty() {
            // Only if not root! Avoids path spec / warning
            // and mapname conversion failed error on unzip
            #[allow(deprecated)]
            zip.add_directory_from_path(name, options)?;
        }
    }
    zip.finish()?;
    Ok(())
}

pub fn zip_directory<T1: AsRef<str>, T2: AsRef<str>>(
    src_dir: T1,
    dst_file: T2,
    zip_options: ZipOptions,
) -> Result<(), UtilError> {
    if !Path::new(src_dir.as_ref()).is_dir() {
        return Err(UtilError::SourceDirectoryNotFound {
            path: src_dir.as_ref().to_string(),
        });
    }

    let path = Path::new(dst_file.as_ref());
    let file = File::create(path).map_err(|e| UtilError::DestinationFileCreate {
        path: path.display().to_string(),
        source: e,
    })?;

    let walkdir = WalkDir::new(src_dir.as_ref());
    let it = walkdir.into_iter();

    zip_dir(&mut it.filter_map(|e| e.ok()), file, zip_options)?;

    Ok(())
}
