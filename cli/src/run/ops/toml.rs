use crate::errors::MycoError;
use crate::impl_from_v8_struct;
use crate::register_sync_op;
use crate::run::ops::macros::sync_op;
use serde_json;
use v8;

struct TomlStringArg {
    toml_string: String,
}

impl_from_v8_struct!(TomlStringArg {
    toml_string: String
});

struct ValueArg {
    value: serde_json::Value,
}

impl_from_v8_struct!(ValueArg {
    value: serde_json::Value
});

pub fn register_toml_ops(
    scope: &mut v8::PinScope<'_, '_>,
    myco_ops: &v8::Object,
) -> Result<(), MycoError> {
    register_sync_op!(scope, myco_ops, "toml_parse", sync_op_toml_parse);
    register_sync_op!(scope, myco_ops, "toml_stringify", sync_op_toml_stringify);

    Ok(())
}

fn sync_op_toml_parse<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    args: v8::FunctionCallbackArguments<'s>,
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

fn sync_op_toml_stringify<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    args: v8::FunctionCallbackArguments<'s>,
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
