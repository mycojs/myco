use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use anyhow::anyhow;
use futures::FutureExt;

use util::transpile::TranspiledFile;

pub struct MycoModuleLoader {
    source_maps: Rc<RefCell<HashMap<PathBuf, Vec<u8>>>>,
}

impl MycoModuleLoader {
    pub fn new() -> Self {
        Self {
            source_maps: Rc::new(RefCell::new(HashMap::new())),
        }
    }
}

impl deno_core::SourceMapGetter for MycoModuleLoader {
    fn get_source_map(&self, file_name: &str) -> Option<Vec<u8>> {
        #[cfg(windows)]
            let file_name = file_name.trim_start_matches("file:///");
        #[cfg(unix)]
            let file_name = file_name.trim_start_matches("file://");
        self.source_maps.borrow().get(Path::new(file_name)).cloned()
    }

    fn get_source_line(&self, file_name: &str, line_number: usize) -> Option<String> {
        if file_name.starts_with("myco:") {
            return None;
        }
        #[cfg(windows)]
            let file_name = file_name.trim_start_matches("file:///");
        #[cfg(unix)]
            let file_name = file_name.trim_start_matches("file://");
        let path = Path::new(file_name);
        let source = std::fs::read_to_string(path).expect("Failed to read file");
        let lines = source.lines().collect::<Vec<_>>();
        lines.get(line_number).map(|s| s.to_string())
    }
}

impl deno_core::ModuleLoader for MycoModuleLoader {
    fn resolve(
        &self,
        specifier: &str,
        referrer: &str,
        _kind: deno_core::ResolutionKind,
    ) -> Result<deno_core::ModuleSpecifier, deno_core::error::AnyError> {
        if specifier.starts_with("myco:") {
            return Ok(deno_core::ModuleSpecifier::parse(specifier)?);
        }

        let specifier_path = PathBuf::from(specifier);

        let path = if specifier_path.is_relative() {
            let base_path = if specifier_path.starts_with(".") || specifier_path.starts_with("..") {
                #[cfg(windows)]
                    let referrer = referrer.trim_start_matches("file:///");
                #[cfg(unix)]
                    let referrer = referrer.trim_start_matches("file://");
                let referrer = PathBuf::from(referrer);
                if referrer.starts_with("myco:") {
                    PathBuf::from(".")
                } else {
                    referrer.parent().expect("referrer must have a parent").to_path_buf()
                }
            } else {
                PathBuf::from(".")
            };
            let relative_path = base_path.join(specifier_path);
            std::env::current_dir().unwrap().join(relative_path)
        } else {
            specifier_path
        };

        let is_directory = path.is_dir();

        let path = if is_directory {
            path.join("index.ts")
        } else {
            path
        };

        let path = if let None = path.extension() {
            path.with_extension("ts")
        } else {
            path
        };

        return if !path.exists() {
            Err(anyhow!("File not found: {}", path.display()).into())
        } else {
            Ok(deno_core::ModuleSpecifier::from_file_path(path).unwrap())
        };
    }

    fn load(
        &self,
        module_specifier: &deno_core::ModuleSpecifier,
        _maybe_referrer: Option<&deno_core::ModuleSpecifier>,
        _is_dyn_import: bool,
    ) -> std::pin::Pin<Box<deno_core::ModuleSourceFuture>> {
        let module_specifier = module_specifier.clone();
        let source_maps = self.source_maps.clone();
        async move {
            let path = module_specifier.to_file_path().unwrap();

            let (module_type, should_transpile) = match FileType::from_path(&path) {
                FileType::JavaScript => (deno_core::ModuleType::JavaScript, false),
                FileType::TypeScript => (deno_core::ModuleType::JavaScript, true),
                FileType::Json => (deno_core::ModuleType::Json, false),
                _ => panic!("Unknown extension {:?}", path.extension()),
            };

            let code = if should_transpile {
                let TranspiledFile {
                    source_map,
                    source
                } = util::transpile::parse_and_gen(&module_specifier)?;
                source_maps.borrow_mut().insert(path, source_map);
                source
            } else {
                std::fs::read_to_string(&path)?
            };
            let module = deno_core::ModuleSource::new(
                module_type,
                deno_core::ModuleCode::from(code),
                &module_specifier,
            );
            Ok(module)
        }
            .boxed_local()
    }
}

enum FileType {
    Unknown,
    TypeScript,
    JavaScript,
    Json,
}

impl FileType {
    pub fn from_path(path: &Path) -> Self {
        match path.extension() {
            None => Self::Unknown,
            Some(os_str) => {
                let lowercase_str = os_str.to_str().map(|s| s.to_lowercase());
                match lowercase_str.as_deref() {
                    | Some("ts")
                    | Some("mts")
                    | Some("cts")
                    | Some("tsx") => Self::TypeScript,
                    | Some("js")
                    | Some("jsx")
                    | Some("mjs")
                    | Some("cjs") => Self::JavaScript,
                    Some("json") => Self::Json,
                    _ => Self::Unknown,
                }
            }
        }
    }
}
