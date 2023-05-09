use std::path::PathBuf;
use deno_ast::{MediaType, ParseParams, SourceTextInfo};
use deno_core::anyhow::anyhow;
use deno_core::futures::FutureExt;

pub struct MycoModuleLoader;

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
            let referrer = referrer.trim_start_matches("file://");
            let referrer = PathBuf::from(referrer);
            let base_path = if referrer.starts_with("myco:") {
                PathBuf::from(".")
            } else {
                referrer.parent().expect("referrer must have a parent").to_path_buf()
            };

            base_path.join(specifier)
        } else {
            specifier_path
        };

        let is_directory = path.is_dir();

        let path = if is_directory {
            path.join("index.ts")
        } else {
            path
        };

        let path = if !path.exists() {
            path.with_extension("ts")
        } else {
            path
        };

        let path = path.canonicalize()?;

        return if !path.exists() {
            Err(anyhow!("File not found: {}", path.display()).into())
        } else {
            Ok(deno_core::ModuleSpecifier::from_file_path(path).unwrap())
        }
    }

    fn load(
        &self,
        module_specifier: &deno_core::ModuleSpecifier,
        _maybe_referrer: Option<&deno_core::ModuleSpecifier>,
        _is_dyn_import: bool,
    ) -> std::pin::Pin<Box<deno_core::ModuleSourceFuture>> {
        let module_specifier = module_specifier.clone();
        async move {
            let path = module_specifier.to_file_path().unwrap();

            let media_type = MediaType::from_path(&path);
            let (module_type, should_transpile) = match MediaType::from_path(&path) {
                MediaType::JavaScript | MediaType::Mjs | MediaType::Cjs => {
                    (deno_core::ModuleType::JavaScript, false)
                }
                MediaType::Jsx => (deno_core::ModuleType::JavaScript, true),
                MediaType::TypeScript
                | MediaType::Mts
                | MediaType::Cts
                | MediaType::Dts
                | MediaType::Dmts
                | MediaType::Dcts
                | MediaType::Tsx => (deno_core::ModuleType::JavaScript, true),
                MediaType::Json => (deno_core::ModuleType::Json, false),
                _ => panic!("Unknown extension {:?}", path.extension()),
            };

            let code = std::fs::read_to_string(&path)?;
            let code = if should_transpile {
                let parsed = deno_ast::parse_module(ParseParams {
                    specifier: module_specifier.to_string(),
                    text_info: SourceTextInfo::from_string(code),
                    media_type,
                    capture_tokens: false,
                    scope_analysis: false,
                    maybe_syntax: None,
                })?;
                parsed.transpile(&Default::default())?.text
            } else {
                code
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
