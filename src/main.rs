use deno_core::error::AnyError;
use deno_core::{ModuleSpecifier, op};
use deno_core::Extension;
use deno_core::Snapshot;
use std::rc::Rc;
use std::env;
use std::path::{PathBuf};
use typescript::TsModuleLoader;

mod typescript;

#[op]
async fn op_read_file(path: String) -> Result<String, AnyError> {
    let contents = tokio::fs::read_to_string(path).await?;
    Ok(contents)
}

#[op]
async fn op_write_file(path: String, contents: String) -> Result<(), AnyError> {
    tokio::fs::write(path, contents).await?;
    Ok(())
}

#[op]
async fn op_fetch(url: String) -> Result<String, AnyError> {
    let body = reqwest::get(url).await?.text().await?;
    Ok(body)
}

#[op]
async fn op_set_timeout(delay: u64) -> Result<(), AnyError> {
    tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
    Ok(())
}

#[op]
async fn op_remove_file(path: String) -> Result<(), AnyError> {
    tokio::fs::remove_file(path).await?;
    Ok(())
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
            op_read_file::decl(),
            op_write_file::decl(),
            op_remove_file::decl(),
            op_fetch::decl(),
            op_set_timeout::decl(),
        ])
        .build();
    let mut js_runtime = deno_core::JsRuntime::new(deno_core::RuntimeOptions {
        module_loader: Some(Rc::new(TsModuleLoader)),
        startup_snapshot: Some(Snapshot::Static(RUNTIME_SNAPSHOT)),
        extensions: vec![myco_extension],
        ..Default::default()
    });

    let user_module_path = PathBuf::from(file_name).canonicalize().expect("Failed to canonicalize user module path");
    let main_module_specifier = ModuleSpecifier::parse("file:///main").expect("Failed to parse main module specifier");
    let main_module_contents = MAIN_JS.replace("{{USER_MODULE}}", &user_module_path.to_string_lossy());
    let main_module_id = js_runtime.load_main_module(&main_module_specifier, Some(main_module_contents)).await?;
    let result = js_runtime.mod_evaluate(main_module_id);
    js_runtime.run_event_loop(false).await?;
    result.await?
}

fn main() {
    let args: Vec<String> = std::env::args().collect();

    if args.is_empty() {
        eprintln!("Usage: myco <file>");
        std::process::exit(1);
    }
    let default = "js/example.ts".to_string();
    let file_path = &args.get(1).unwrap_or(&default);

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    if let Err(error) = runtime.block_on(run_js(file_path)) {
        eprintln!("error: {error}");
    }
}
