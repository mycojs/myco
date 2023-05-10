use std::fs;
use std::path::Path;

use base64::Engine;
use deno_core::error::AnyError;
use deno_core::ModuleSpecifier;
use swc_common::{FileName, FilePathMapping, Globals, GLOBALS, Mark, SourceMap};
use swc_common::comments::SingleThreadedComments;
use swc_common::errors::{ColorConfig, Handler};
use swc_common::input::SourceFileInput;
use swc_common::sync::Lrc;
use swc_ecma_ast::*;
use swc_ecma_codegen::{Emitter, text_writer::JsWriter};
use swc_ecma_parser::{lexer::Lexer, Parser, Syntax, TsConfig};
use swc_ecma_transforms_base::{fixer::fixer, hygiene::hygiene, resolver};
use swc_ecma_transforms_typescript::strip;
use swc_ecma_visit::FoldWith;

const BASE64_ENGINE: base64::engine::GeneralPurpose =
    base64::engine::GeneralPurpose::new(&base64::alphabet::STANDARD, base64::engine::general_purpose::PAD);

pub struct TranspiledFile {
    pub source: String,
    pub source_map: Vec<u8>,
}

pub fn parse_and_gen(module_specifier: &ModuleSpecifier) -> Result<TranspiledFile, AnyError> {
    let cm = Lrc::new(SourceMap::new(FilePathMapping::empty()));
    let handler = Handler::with_tty_emitter(ColorConfig::Auto, true, false, Some(cm.clone()));
    let path = module_specifier.to_file_path().unwrap();
    let source = fs::read_to_string(&path).unwrap();
    let fm = cm.new_source_file(FileName::Url(module_specifier.clone()), source);

    let comments = SingleThreadedComments::default();

    let lexer = Lexer::new(
        Syntax::Typescript(TsConfig {
            tsx: path.ends_with(".tsx"),
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

    let module = parser
        .parse_module()
        .map_err(|e| e.into_diagnostic(&handler).emit())
        .expect("failed to parse module.");

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

            emitter.emit_module(&module).unwrap();
        }

        let source_map_vec = cm.build_source_map(&source_map);
        let mut source_map = vec![];
        source_map_vec.to_writer(&mut source_map).unwrap();

        let mut source = String::from_utf8(code)?;
        Ok(TranspiledFile {
            source,
            source_map,
        })
    })
}
