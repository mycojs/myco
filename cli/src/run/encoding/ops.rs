use deno_core::{op, OpState, ZeroCopyBuf};
use crate::AnyError;

#[op]
pub fn myco_op_encode_utf8_sync(_state: &mut OpState, text: String) -> Result<ZeroCopyBuf, AnyError> {
    let buffer = text.as_bytes().to_vec();
    Ok(buffer.into())
}

#[op]
pub fn myco_op_decode_utf8_sync(_state: &mut OpState, buffer: ZeroCopyBuf) -> Result<String, AnyError> {
    let text = String::from_utf8(buffer.to_vec())?;
    Ok(text)
}
