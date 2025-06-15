use crate::errors::MycoError;
use crate::register_sync_op;
use crate::run::ops::macros::sync_op;
use serde::Deserialize;
use serde_json;
use v8;

#[derive(Deserialize)]
struct TomlStringArg {
    toml_string: String,
}

#[derive(Deserialize)]
struct ValueArg {
    value: serde_json::Value,
}

pub fn register_toml_ops(
    scope: &mut v8::ContextScope<v8::HandleScope>,
    myco_ops: &v8::Object,
) -> Result<(), MycoError> {
    register_sync_op!(scope, myco_ops, "toml_parse", sync_op_toml_parse);
    register_sync_op!(scope, myco_ops, "toml_stringify", sync_op_toml_stringify);

    Ok(())
}

fn sync_op_toml_parse(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    rv: v8::ReturnValue,
) {
    sync_op(
        scope,
        &args,
        rv,
        |_scope, input: TomlStringArg| -> Result<serde_json::Value, MycoError> {
            toml::from_str::<serde_json::Value>(&input.toml_string).map_err(|e| {
                MycoError::Internal {
                    message: format!("Failed to parse TOML: {}", e),
                }
            })
        },
    );
}

fn sync_op_toml_stringify(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    rv: v8::ReturnValue,
) {
    sync_op(
        scope,
        &args,
        rv,
        |_scope, input: ValueArg| -> Result<String, MycoError> {
            toml::to_string(&input.value).map_err(|e| MycoError::Internal {
                message: format!("Failed to stringify value as TOML: {}", e),
            })
        },
    );
}
