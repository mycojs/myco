use std::cell::RefCell;
use deno_core::error::AnyError;
use deno_core::{ModuleSpecifier, op, OpState};
use deno_core::Extension;
use deno_core::Snapshot;
use std::rc::Rc;
use std::env;
use std::path::PathBuf;
use deno_core::anyhow::anyhow;
use typescript::TsModuleLoader;
use capabilities::{Capability, CapabilityRegistry};

mod typescript;
mod capabilities;

type Token = String;

fn create_token(state: Rc<RefCell<OpState>>, capability: Capability) -> Token {
    let mut state = state.borrow_mut();
    let mut registry = state.borrow_mut::<CapabilityRegistry>();
    registry.register(capability)
}

#[op]
async fn op_request_read_file(state: Rc<RefCell<OpState>>, path: String) -> Result<Token, AnyError> {
    Ok(create_token(state, Capability::ReadFile(path)))
}

#[op]
async fn op_request_write_file(state: Rc<RefCell<OpState>>, path: String) -> Result<Token, AnyError> {
    Ok(create_token(state, Capability::WriteFile(path)))
}

#[op]
async fn op_request_read_dir(state: Rc<RefCell<OpState>>, path: String) -> Result<Token, AnyError> {
    Ok(create_token(state, Capability::ReadDir(path)))
}

#[op]
async fn op_request_write_dir(state: Rc<RefCell<OpState>>, path: String) -> Result<Token, AnyError> {
    Ok(create_token(state, Capability::WriteDir(path)))
}

#[op]
async fn op_request_fetch_url(state: Rc<RefCell<OpState>>, url: String) -> Result<Token, AnyError> {
    Ok(create_token(state, Capability::FetchUrl(url)))
}

#[op]
async fn op_request_fetch_prefix(state: Rc<RefCell<OpState>>, prefix: String) -> Result<Token, AnyError> {
    Ok(create_token(state, Capability::FetchPrefix(prefix)))
}

#[op]
async fn op_read_file(state: Rc<RefCell<OpState>>, token: Token) -> Result<String, AnyError> {
    let state = state.borrow();
    let registry = state.borrow::<CapabilityRegistry>();
    let path = match registry.get(&token) {
        Some(Capability::ReadFile(path)) => path,
        _ => return Err(anyhow!("Invalid token")),
    };
    let contents = tokio::fs::read_to_string(path).await?;
    Ok(contents)
}

#[op]
async fn op_write_file(state: Rc<RefCell<OpState>>, token: Token, contents: String) -> Result<(), AnyError> {
    let state = state.borrow();
    let registry = state.borrow::<CapabilityRegistry>();
    let path = match registry.get(&token) {
        Some(Capability::WriteFile(path)) => path,
        _ => return Err(anyhow!("Invalid token")),
    };
    tokio::fs::write(path, contents).await?;
    Ok(())
}

#[op]
async fn op_remove_file(state: Rc<RefCell<OpState>>, token: Token) -> Result<(), AnyError> {
    let state = state.borrow();
    let registry = state.borrow::<CapabilityRegistry>();
    let path = match registry.get(&token) {
        Some(Capability::WriteFile(path)) => path,
        _ => return Err(anyhow!("Invalid token")),
    };
    tokio::fs::remove_file(path).await?;
    Ok(())
}

#[op]
async fn op_fetch_url(state: Rc<RefCell<OpState>>, token: Token) -> Result<String, AnyError> {
    let state = state.borrow();
    let registry = state.borrow::<CapabilityRegistry>();
    let url = match registry.get(&token) {
        Some(Capability::FetchUrl(url)) => url,
        _ => return Err(anyhow!("Invalid token")),
    };
    let body = reqwest::get(url).await?.text().await?;
    Ok(body)
}

#[op]
async fn op_set_timeout(delay: u64) -> Result<(), AnyError> {
    tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
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
            // Files
            op_request_read_file::decl(),
            op_request_write_file::decl(),
            op_request_read_dir::decl(),
            op_request_write_dir::decl(),
            op_read_file::decl(),
            op_write_file::decl(),
            op_remove_file::decl(),

            // Http
            op_request_fetch_url::decl(),
            op_request_fetch_prefix::decl(),
            op_fetch_url::decl(),

            // Core
            op_set_timeout::decl(),
        ])
        .state(move |state| {
            state.put(CapabilityRegistry::new());
        })
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
