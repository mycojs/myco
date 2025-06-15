use std::fs;

use swc_common::comments::SingleThreadedComments;
use swc_common::errors::{ColorConfig, Handler};
use swc_common::input::SourceFileInput;
use swc_common::sync::Lrc;
use swc_common::{FileName, FilePathMapping, Globals, Mark, SourceMap, GLOBALS};
use swc_ecma_ast::*;
use swc_ecma_codegen::{text_writer::JsWriter, Emitter};
use swc_ecma_parser::{lexer::Lexer, Parser, Syntax, TsConfig};
use swc_ecma_transforms_base::{fixer::fixer, hygiene::hygiene, resolver};
use swc_ecma_transforms_typescript::strip;
use swc_ecma_visit::FoldWith;
use url::Url;

use crate::UtilError;

pub struct TranspiledFile {
    pub source: String,
    pub source_map: Vec<u8>,
}

pub fn parse_and_gen(module_specifier: &Url) -> Result<TranspiledFile, UtilError> {
    let cm = Lrc::new(SourceMap::new(FilePathMapping::empty()));
    let handler = Handler::with_tty_emitter(ColorConfig::Auto, true, false, Some(cm.clone()));
    let path = module_specifier
        .to_file_path()
        .map_err(|_| UtilError::InvalidUrl {
            message: format!("Cannot convert URL to file path: {}", module_specifier),
        })?;
    let source = fs::read_to_string(&path).map_err(|e| UtilError::FileRead {
        path: path.display().to_string(),
        source: e,
    })?;
    let fm = cm.new_source_file(FileName::Real(path.clone()), source);

    let comments = SingleThreadedComments::default();

    let lexer = Lexer::new(
        Syntax::Typescript(TsConfig {
            tsx: path.to_string_lossy().ends_with(".tsx"),
            ..Default::default()
        }),
        EsVersion::latest(),
        SourceFileInput::from(&*fm),
        Some(&comments),
    );

    let mut parser = Parser::new_from(lexer);

    for e in parser.take_errors() {
        e.into_diagnostic(&handler).emit();
    }

    let module = parser.parse_module().map_err(|e| {
        e.into_diagnostic(&handler).emit();
        UtilError::TypeScriptParsing {
            message: "Failed to parse TypeScript module".to_string(),
        }
    })?;

    let globals = Globals::default();
    GLOBALS.set(&globals, || {
        let unresolved_mark = Mark::new();
        let top_level_mark = Mark::new();

        // Conduct identifier scope analysis
        let module = module.fold_with(&mut resolver(unresolved_mark, top_level_mark, true));

        // Remove typescript types
        let module = module.fold_with(&mut strip(top_level_mark));

        // Fix up any identifiers with the same name, but different contexts
        let module = module.fold_with(&mut hygiene());

        // Ensure that we have enough parenthesis.
        let module = module.fold_with(&mut fixer(Some(&comments)));

        let mut code = vec![];
        let mut source_map = vec![];
        {
            let mut emitter = Emitter {
                cfg: Default::default(),
                cm: cm.clone(),
                comments: None,
                wr: JsWriter::new(cm.clone(), "\n", &mut code, Some(&mut source_map)),
            };

            emitter
                .emit_module(&module)
                .map_err(|_| UtilError::CodeGeneration {
                    message: "Failed to emit JavaScript code".to_string(),
                })?;
        }

        let source_map_vec = cm.build_source_map(&source_map);
        let mut source_map = vec![];
        source_map_vec
            .to_writer(&mut source_map)
            .map_err(|_| UtilError::SourceMapGeneration {
                message: "Failed to generate source map".to_string(),
            })?;

        let source = String::from_utf8(code)?;
        Ok(TranspiledFile { source, source_map })
    })
}

pub fn parse_and_gen_path(path: &std::path::Path) -> Result<TranspiledFile, UtilError> {
    let url = url::Url::from_file_path(path).map_err(|_| UtilError::InvalidFilePath {
        path: path.display().to_string(),
    })?;
    parse_and_gen(&url)
}
