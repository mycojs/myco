use std::path::PathBuf;
use std::rc::Rc;

use deno_core::{Extension, ModuleCode, ModuleSpecifier, Snapshot};

use loader::MycoModuleLoader;
pub use token::*;

use crate::AnyError;
use crate::manifest::MycoToml;

#[macro_use]
mod token;
mod filesystem;
mod network;
mod time;
mod loader;
mod env;
mod encoding;

pub fn run(myco_toml: &MycoToml, script: &String) {
    if let Some(run) = &myco_toml.run {
        if let Some(script) = run.get(script) {
            run_file(script);
        } else {
            run_file(script);
        }
    } else {
        run_file(script);
    };
}

pub fn run_file(file_path: &str) {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    if let Err(error) = runtime.block_on(run_js(file_path)) {
        if let Some(js_error) = error.downcast_ref::<deno_core::error::JsError>() {
            eprintln!("error: {}", js_error);
        } else {
            eprintln!("error: {error}");
            eprintln!("{}", error.backtrace());
        }
    }
}

static RUNTIME_SNAPSHOT: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/MYCO_SNAPSHOT.bin"));


const MAIN_JS: &str = "\
const Myco = globalThis.Myco;

// Delete the global scope that we don't want access to
delete globalThis.Myco;
delete globalThis.Deno;
delete globalThis.__bootstrap;
delete globalThis.queueMicrotask;

const {default: userModule} = await import('{{USER_MODULE}}');

userModule(Myco);
";

async fn run_js(file_name: &str) -> Result<(), AnyError> {
    let myco_extension = Extension::builder("myco")
        .ops(vec![
            // Files
            filesystem::myco_op_request_read_file::decl(),
            filesystem::myco_op_request_write_file::decl(),
            filesystem::myco_op_request_exec_file::decl(),
            filesystem::myco_op_request_read_dir::decl(),
            filesystem::myco_op_request_write_dir::decl(),
            filesystem::myco_op_request_exec_dir::decl(),
            filesystem::myco_op_read_file::decl(),
            filesystem::myco_op_read_file_sync::decl(),
            filesystem::myco_op_stat_file::decl(),
            filesystem::myco_op_stat_file_sync::decl(),
            filesystem::myco_op_list_dir::decl(),
            filesystem::myco_op_list_dir_sync::decl(),
            filesystem::myco_op_write_file::decl(),
            filesystem::myco_op_write_file_sync::decl(),
            filesystem::myco_op_remove_file::decl(),
            filesystem::myco_op_remove_file_sync::decl(),
            filesystem::myco_op_mkdirp::decl(),
            filesystem::myco_op_mkdirp_sync::decl(),
            filesystem::myco_op_rmdir::decl(),
            filesystem::myco_op_rmdir_sync::decl(),
            filesystem::myco_op_exec_file::decl(),
            filesystem::myco_op_exec_file_sync::decl(),

            // Http
            network::myco_op_request_fetch_url::decl(),
            network::myco_op_request_fetch_prefix::decl(),
            network::myco_op_fetch_url::decl(),
            network::myco_op_bind_tcp_listener::decl(),
            network::myco_op_accept_tcp_stream::decl(),
            network::myco_op_read_all_tcp_stream::decl(),
            network::myco_op_write_all_tcp_stream::decl(),
            network::myco_op_close_tcp_stream::decl(),
            network::myco_op_close_tcp_listener::decl(),

            // Encoding
            encoding::myco_op_encode_utf8_sync::decl(),
            encoding::myco_op_decode_utf8_sync::decl(),
            encoding::myco_op_encode_gzip_sync::decl(),
            encoding::myco_op_decode_gzip_sync::decl(),

            // Core
            time::myco_op_set_timeout::decl(),
            env::myco_op_argv_sync::decl(),
        ])
        .state(move |state| {
            state.put(CapabilityRegistry::new());
        })
        .force_op_registration()
        .build();
    let module_loader = Rc::new(MycoModuleLoader::new());
    let mut js_runtime = deno_core::JsRuntime::new(deno_core::RuntimeOptions {
        module_loader: Some(module_loader.clone()),
        startup_snapshot: Some(Snapshot::Static(RUNTIME_SNAPSHOT)),
        extensions: vec![myco_extension],
        source_map_getter: Some(Box::new(module_loader)),
        ..Default::default()
    });

    let user_module_path = PathBuf::from(file_name);
    let main_module_specifier = ModuleSpecifier::parse("myco:main").expect("Failed to parse main module specifier");
    let main_module_contents = MAIN_JS.replace("{{USER_MODULE}}", &user_module_path.to_string_lossy());
    let main_module_id = js_runtime.load_main_module(&main_module_specifier, Some(ModuleCode::from(main_module_contents))).await?;
    let result = js_runtime.mod_evaluate(main_module_id);
    js_runtime.run_event_loop(false).await?;
    result.await?
}
