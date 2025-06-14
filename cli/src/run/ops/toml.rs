use v8;
use serde_json;
use crate::errors::MycoError;
use crate::register_sync_op;

pub fn register_toml_ops(scope: &mut v8::ContextScope<v8::HandleScope>, myco_ops: &v8::Object) -> Result<(), MycoError> {
    register_sync_op!(scope, myco_ops, "toml_parse", sync_op_toml_parse);
    register_sync_op!(scope, myco_ops, "toml_stringify", sync_op_toml_stringify);
    
    Ok(())
}

fn sync_op_toml_parse<'a>(
    scope: &mut v8::HandleScope<'a>,
    args: v8::FunctionCallbackArguments<'a>,
    mut rv: v8::ReturnValue,
) {
    if args.length() < 1 {
        let error = v8::String::new(scope, "toml_parse_sync requires 1 argument: toml_string").unwrap();
        scope.throw_exception(error.into());
        return;
    }
    
    let toml_arg = args.get(0);
    if !toml_arg.is_string() {
        let error = v8::String::new(scope, "toml_parse_sync: argument must be a string").unwrap();
        scope.throw_exception(error.into());
        return;
    }
    
    let toml_string = toml_arg.to_rust_string_lossy(scope);
    
    // Parse TOML to serde_json::Value first, then convert to V8
    match toml::from_str::<serde_json::Value>(&toml_string) {
        Ok(value) => {
            // Convert serde_json::Value to V8 value
            match serde_json_to_v8(scope, &value) {
                Ok(v8_value) => {
                    rv.set(v8_value);
                },
                Err(e) => {
                    let error_msg = format!("Failed to convert parsed TOML to JavaScript value: {}", e);
                    let error = v8::String::new(scope, &error_msg).unwrap();
                    scope.throw_exception(error.into());
                }
            }
        },
        Err(e) => {
            let error_msg = format!("Failed to parse TOML: {}", e);
            let error = v8::String::new(scope, &error_msg).unwrap();
            scope.throw_exception(error.into());
        }
    }
}

fn sync_op_toml_stringify<'a>(
    scope: &mut v8::HandleScope<'a>,
    args: v8::FunctionCallbackArguments<'a>,
    mut rv: v8::ReturnValue,
) {
    if args.length() < 1 {
        let error = v8::String::new(scope, "toml_stringify_sync requires 1 argument: value").unwrap();
        scope.throw_exception(error.into());
        return;
    }
    
    let value_arg = args.get(0);
    
    // Convert V8 value to serde_json::Value first
    match v8_to_serde_json(scope, value_arg) {
        Ok(json_value) => {
            // Convert serde_json::Value to TOML string
            match toml::to_string(&json_value) {
                Ok(toml_string) => {
                    let result = v8::String::new(scope, &toml_string).unwrap();
                    rv.set(result.into());
                },
                Err(e) => {
                    let error_msg = format!("Failed to stringify value as TOML: {}", e);
                    let error = v8::String::new(scope, &error_msg).unwrap();
                    scope.throw_exception(error.into());
                }
            }
        },
        Err(e) => {
            let error_msg = format!("Failed to convert JavaScript value to TOML-compatible format: {}", e);
            let error = v8::String::new(scope, &error_msg).unwrap();
            scope.throw_exception(error.into());
        }
    }
}

// Helper function to convert serde_json::Value to V8 value
fn serde_json_to_v8<'a>(scope: &mut v8::HandleScope<'a>, value: &serde_json::Value) -> Result<v8::Local<'a, v8::Value>, MycoError> {
    match value {
        serde_json::Value::Null => Ok(v8::null(scope).into()),
        serde_json::Value::Bool(b) => Ok(v8::Boolean::new(scope, *b).into()),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Ok(v8::Number::new(scope, i as f64).into())
            } else if let Some(f) = n.as_f64() {
                Ok(v8::Number::new(scope, f).into())
            } else {
                Err(MycoError::Internal { 
                    message: "Invalid number format".to_string() 
                })
            }
        },
        serde_json::Value::String(s) => {
            Ok(v8::String::new(scope, s).unwrap().into())
        },
        serde_json::Value::Array(arr) => {
            let v8_array = v8::Array::new(scope, arr.len() as i32);
            for (i, item) in arr.iter().enumerate() {
                let v8_item = serde_json_to_v8(scope, item)?;
                v8_array.set_index(scope, i as u32, v8_item);
            }
            Ok(v8_array.into())
        },
        serde_json::Value::Object(obj) => {
            let v8_object = v8::Object::new(scope);
            for (key, val) in obj.iter() {
                let v8_key = v8::String::new(scope, key).unwrap();
                let v8_val = serde_json_to_v8(scope, val)?;
                v8_object.set(scope, v8_key.into(), v8_val);
            }
            Ok(v8_object.into())
        }
    }
}

// Helper function to convert V8 value to serde_json::Value
fn v8_to_serde_json<'a>(scope: &mut v8::HandleScope<'a>, value: v8::Local<'a, v8::Value>) -> Result<serde_json::Value, MycoError> {
    if value.is_null() || value.is_undefined() {
        Ok(serde_json::Value::Null)
    } else if value.is_boolean() {
        Ok(serde_json::Value::Bool(value.boolean_value(scope)))
    } else if value.is_number() {
        let num = value.number_value(scope).unwrap_or(0.0);
        if num.fract() == 0.0 && num >= i64::MIN as f64 && num <= i64::MAX as f64 {
            Ok(serde_json::Value::Number((num as i64).into()))
        } else {
            Ok(serde_json::json!(num))
        }
    } else if value.is_string() {
        let string = value.to_rust_string_lossy(scope);
        Ok(serde_json::Value::String(string))
    } else if value.is_array() {
        if let Ok(array) = v8::Local::<v8::Array>::try_from(value) {
            let mut result = Vec::new();
            let length = array.length();
            for i in 0..length {
                if let Some(item) = array.get_index(scope, i) {
                    result.push(v8_to_serde_json(scope, item)?);
                }
            }
            Ok(serde_json::Value::Array(result))
        } else {
            Err(MycoError::Internal { 
                message: "Failed to convert to array".to_string() 
            })
        }
    } else if value.is_object() {
        if let Ok(object) = v8::Local::<v8::Object>::try_from(value) {
            let mut result = serde_json::Map::new();
            
            if let Some(prop_names) = object.get_own_property_names(scope, v8::GetPropertyNamesArgs::default()) {
                let length = prop_names.length();
                
                for i in 0..length {
                    if let Some(key_val) = prop_names.get_index(scope, i) {
                        let key = key_val.to_rust_string_lossy(scope);
                        if let Some(val) = object.get(scope, key_val) {
                            result.insert(key, v8_to_serde_json(scope, val)?);
                        }
                    }
                }
            }
            
            Ok(serde_json::Value::Object(result))
        } else {
            Err(MycoError::Internal { 
                message: "Failed to convert to object".to_string() 
            })
        }
    } else {
        Err(MycoError::Internal { 
            message: "Unsupported value type for TOML conversion".to_string() 
        })
    }
} 