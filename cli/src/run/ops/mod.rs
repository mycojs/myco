pub mod console;
pub mod encoding;
pub mod filesystem;
pub mod http;
pub mod macros;
pub mod time;
pub mod toml;

use crate::errors::MycoError;
use log::{debug, info, trace};

pub fn register_ops(
    scope: &mut v8::ContextScope<v8::HandleScope>,
    global: &v8::Object,
) -> Result<(), MycoError> {
    info!("Registering JavaScript runtime operations");

    // Create the Myco object
    debug!("Creating Myco global object");
    let myco_obj = v8::Object::new(scope);

    // Create MycoOps object for low-level operations
    debug!("Creating MycoOps object for low-level operations");
    let myco_ops = v8::Object::new(scope);

    // Add sync and async keys to MycoOps as objects
    trace!("Setting up sync and async operation namespaces");
    let sync_obj = v8::Object::new(scope);
    let async_obj = v8::Object::new(scope);
    let sync_key = v8::String::new(scope, "sync").unwrap();
    let async_key = v8::String::new(scope, "async").unwrap();
    myco_ops.set(scope, sync_key.into(), sync_obj.into());
    myco_ops.set(scope, async_key.into(), async_obj.into());

    // Register console operations
    debug!("Registering console operations");
    console::register_console_ops(scope, &myco_ops)?;

    // Register encoding operations
    debug!("Registering encoding operations");
    encoding::register_encoding_ops(scope, &myco_ops)?;

    // Register TOML operations
    debug!("Registering TOML operations");
    toml::register_toml_ops(scope, &myco_ops)?;

    // Register time operations
    debug!("Registering time operations");
    time::register_time_ops(scope, &myco_ops)?;

    // Register filesystem operations
    debug!("Registering filesystem operations");
    filesystem::register_filesystem_ops(scope, &myco_ops)?;

    // Register HTTP operations
    debug!("Registering HTTP client operations");
    http::client::register_http_client_ops(scope, &myco_ops)?;

    // Set argv property on Myco object
    debug!("Setting up command line arguments");
    let argv: Vec<String> = std::env::args().collect();
    trace!("Command line arguments: {:?}", argv);
    let v8_array = v8::Array::new(scope, argv.len() as i32);
    for (i, arg) in argv.iter().enumerate() {
        let v8_string = v8::String::new(scope, arg).ok_or(MycoError::V8StringCreation)?;
        v8_array.set_index(scope, i as u32, v8_string.into());
    }
    let argv_key = v8::String::new(scope, "argv").ok_or(MycoError::V8StringCreation)?;
    myco_obj.set(scope, argv_key.into(), v8_array.into());

    // Set Myco object on global
    debug!("Setting Myco object on global scope");
    let myco_key = v8::String::new(scope, "Myco").ok_or(MycoError::V8StringCreation)?;
    global.set(scope, myco_key.into(), myco_obj.into());

    // Set MycoOps object on global (will be captured and deleted by runtime)
    debug!("Setting MycoOps object on global scope");
    let myco_ops_key = v8::String::new(scope, "MycoOps").ok_or(MycoError::V8StringCreation)?;
    global.set(scope, myco_ops_key.into(), myco_ops.into());

    info!("All JavaScript runtime operations registered successfully");
    Ok(())
}
