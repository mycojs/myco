use crate::errors::MycoError;
use crate::impl_from_v8_struct;
use crate::register_sync_op;
use crate::run::ops::convert::{JsBuffer, ToJsBuffer};
use crate::run::ops::macros::sync_op;
use v8;

struct TextArg {
    text: String,
}

impl_from_v8_struct!(TextArg { text: String });

struct BytesArg {
    bytes: JsBuffer,
}

impl_from_v8_struct!(BytesArg { bytes: JsBuffer });

pub fn register_encoding_ops(
    scope: &mut v8::PinScope<'_, '_>,
    myco_ops: &v8::Object,
) -> Result<(), MycoError> {
    register_sync_op!(scope, myco_ops, "encode_utf8", sync_op_encode_utf8);
    register_sync_op!(scope, myco_ops, "decode_utf8", sync_op_decode_utf8);

    Ok(())
}

fn sync_op_encode_utf8<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    args: v8::FunctionCallbackArguments<'s>,
    rv: v8::ReturnValue,
) {
    sync_op(
        scope,
        &args,
        rv,
        |_scope, input: TextArg| -> Result<ToJsBuffer, MycoError> {
            let bytes = input.text.as_bytes();
            Ok(ToJsBuffer::from(bytes.to_vec()))
        },
    );
}

fn sync_op_decode_utf8<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    args: v8::FunctionCallbackArguments<'s>,
    rv: v8::ReturnValue,
) {
    sync_op(
        scope,
        &args,
        rv,
        |_scope, input: BytesArg| -> Result<String, MycoError> {
            match std::str::from_utf8(&input.bytes) {
                Ok(text) => Ok(text.to_string()),
                Err(_) => Err(MycoError::Internal {
                    message: "Invalid UTF-8 sequence".to_string(),
                }),
            }
        },
    );
}
