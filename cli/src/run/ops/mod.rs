pub mod filesystem;
pub mod console;
pub mod encoding;
pub mod time;
pub mod macros;
pub mod http;
pub mod toml;

use crate::errors::MycoError;

pub fn register_ops(scope: &mut v8::ContextScope<v8::HandleScope>, global: &v8::Object) -> Result<(), MycoError> {
    // Create the Myco object
    let myco_obj = v8::Object::new(scope);

    // Create MycoOps object for low-level operations
    let myco_ops = v8::Object::new(scope);
    
    // Register console operations
    console::register_console_ops(scope, &myco_ops)?;
    
    // Register encoding operations
    encoding::register_encoding_ops(scope, &myco_ops)?;

    // Register TOML operations
    toml::register_toml_ops(scope, &myco_ops)?;

    // Register time operations
    time::register_time_ops(scope, &myco_ops)?;

    // Register filesystem operations
    filesystem::register_filesystem_ops(scope, &myco_ops)?;
    
    // Register HTTP operations
    http::client::register_http_client_ops(scope, &myco_ops)?;

    // Set argv property on Myco object
    let argv: Vec<String> = std::env::args().collect();
    let v8_array = v8::Array::new(scope, argv.len() as i32);
    for (i, arg) in argv.iter().enumerate() {
        let v8_string = v8::String::new(scope, arg)
            .ok_or(MycoError::V8StringCreation)?;
        v8_array.set_index(scope, i as u32, v8_string.into());
    }
    let argv_key = v8::String::new(scope, "argv")
        .ok_or(MycoError::V8StringCreation)?;
    myco_obj.set(scope, argv_key.into(), v8_array.into());

    // Set Myco object on global
    let myco_key = v8::String::new(scope, "Myco")
        .ok_or(MycoError::V8StringCreation)?;
    global.set(scope, myco_key.into(), myco_obj.into());
    
    // Set MycoOps object on global (will be captured and deleted by runtime)
    let myco_ops_key = v8::String::new(scope, "MycoOps")
        .ok_or(MycoError::V8StringCreation)?;
    global.set(scope, myco_ops_key.into(), myco_ops.into());
    
    Ok(())
}
