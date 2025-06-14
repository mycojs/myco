use v8;
use crate::errors::MycoError;
use crate::register_sync_op;

pub fn register_encoding_ops(scope: &mut v8::ContextScope<v8::HandleScope>, myco_ops: &v8::Object) -> Result<(), MycoError> {
    register_sync_op!(scope, myco_ops, "encode_utf8", sync_op_encode_utf8);
    register_sync_op!(scope, myco_ops, "decode_utf8", sync_op_decode_utf8);
    
    Ok(())
}

fn sync_op_encode_utf8(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    if args.length() < 1 {
        let error = v8::String::new(scope, "encode_utf8_sync requires 1 argument: text").unwrap();
        scope.throw_exception(error.into());
        return;
    }
    
    let text_arg = args.get(0);
    if !text_arg.is_string() {
        let error = v8::String::new(scope, "encode_utf8_sync: argument must be a string").unwrap();
        scope.throw_exception(error.into());
        return;
    }
    
    let text = text_arg.to_rust_string_lossy(scope);
    let bytes = text.as_bytes();
    
    // Create a Uint8Array with the encoded bytes
    let array_buffer = v8::ArrayBuffer::new(scope, bytes.len());
    let backing_store = array_buffer.get_backing_store();
    
    if let Some(data_ptr) = backing_store.data() {
        unsafe {
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), data_ptr.as_ptr() as *mut u8, bytes.len());
        }
    }
    
    let uint8_array = v8::Uint8Array::new(scope, array_buffer, 0, bytes.len()).unwrap();
    rv.set(uint8_array.into());
}

fn sync_op_decode_utf8(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    if args.length() < 1 {
        let error = v8::String::new(scope, "decode_utf8_sync requires 1 argument: bytes").unwrap();
        scope.throw_exception(error.into());
        return;
    }
    
    let bytes_arg = args.get(0);
    
    // Check if it's a Uint8Array or similar typed array
    if !bytes_arg.is_uint8_array() && !bytes_arg.is_array_buffer() && !bytes_arg.is_array_buffer_view() {
        let error = v8::String::new(scope, "decode_utf8_sync: argument must be a Uint8Array or ArrayBuffer").unwrap();
        scope.throw_exception(error.into());
        return;
    }
    
    let bytes = if let Ok(uint8_array) = v8::Local::<v8::Uint8Array>::try_from(bytes_arg) {
        // Handle Uint8Array
        let byte_length = uint8_array.byte_length();
        
        // Handle empty arrays
        if byte_length == 0 {
            &[]
        } else {
            let array_buffer = uint8_array.buffer(scope).unwrap();
            let backing_store = array_buffer.get_backing_store();
            let byte_offset = uint8_array.byte_offset();
            
            if let Some(data_ptr) = backing_store.data() {
                unsafe {
                    std::slice::from_raw_parts((data_ptr.as_ptr() as *const u8).add(byte_offset), byte_length)
                }
            } else {
                let error = v8::String::new(scope, "decode_utf8_sync: failed to access array buffer data").unwrap();
                scope.throw_exception(error.into());
                return;
            }
        }
    } else if let Ok(array_buffer) = v8::Local::<v8::ArrayBuffer>::try_from(bytes_arg) {
        // Handle ArrayBuffer
        let byte_length = array_buffer.byte_length();
        
        // Handle empty buffers
        if byte_length == 0 {
            &[]
        } else {
            let backing_store = array_buffer.get_backing_store();
            
            if let Some(data_ptr) = backing_store.data() {
                unsafe {
                    std::slice::from_raw_parts(data_ptr.as_ptr() as *const u8, byte_length)
                }
            } else {
                let error = v8::String::new(scope, "decode_utf8_sync: failed to access array buffer data").unwrap();
                scope.throw_exception(error.into());
                return;
            }
        }
    } else {
        let error = v8::String::new(scope, "decode_utf8_sync: unsupported array type").unwrap();
        scope.throw_exception(error.into());
        return;
    };
    
    match std::str::from_utf8(bytes) {
        Ok(text) => {
            let result = v8::String::new(scope, text).unwrap();
            rv.set(result.into());
        },
        Err(_) => {
            let error = v8::String::new(scope, "decode_utf8_sync: invalid UTF-8 sequence").unwrap();
            scope.throw_exception(error.into());
        }
    }
} 