use v8;
use serde::Deserialize;
use crate::errors::MycoError;
use crate::run::ops::macros::sync_op;
use crate::register_sync_op;

#[derive(Deserialize)]
struct TextArg {
    text: String,
}

#[derive(Deserialize)]
struct BytesArg {
    bytes: serde_v8::JsBuffer,
}

pub fn register_encoding_ops(scope: &mut v8::ContextScope<v8::HandleScope>, myco_ops: &v8::Object) -> Result<(), MycoError> {
    register_sync_op!(scope, myco_ops, "encode_utf8", sync_op_encode_utf8);
    register_sync_op!(scope, myco_ops, "decode_utf8", sync_op_decode_utf8);
    
    Ok(())
}

fn sync_op_encode_utf8(scope: &mut v8::HandleScope, args: v8::FunctionCallbackArguments, rv: v8::ReturnValue) {
    sync_op(scope, &args, rv, |_scope, input: TextArg| -> Result<serde_v8::ToJsBuffer, MycoError> {
        let bytes = input.text.as_bytes();
        Ok(serde_v8::ToJsBuffer::from(bytes.to_vec()))
    });
}

fn sync_op_decode_utf8(scope: &mut v8::HandleScope, args: v8::FunctionCallbackArguments, rv: v8::ReturnValue) {
    sync_op(scope, &args, rv, |_scope, input: BytesArg| -> Result<String, MycoError> {
        match std::str::from_utf8(&input.bytes) {
            Ok(text) => Ok(text.to_string()),
            Err(_) => Err(MycoError::Internal {
                message: "Invalid UTF-8 sequence".to_string()
            })
        }
    });
} 