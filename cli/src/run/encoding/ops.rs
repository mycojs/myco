use std::io::{Read, Write};
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

#[op]
pub fn myco_op_decode_gzip_sync(_state: &mut OpState, buffer: ZeroCopyBuf) -> Result<ZeroCopyBuf, AnyError> {
    let mut decoder = flate2::read::GzDecoder::new(buffer.as_ref());
    let mut buffer = Vec::new();
    decoder.read_to_end(&mut buffer)?;
    Ok(buffer.into())
}

#[op]
pub fn myco_op_encode_gzip_sync(_state: &mut OpState, buffer: ZeroCopyBuf) -> Result<ZeroCopyBuf, AnyError> {
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    encoder.write_all(buffer.as_ref())?;
    let buffer = encoder.finish()?;
    Ok(buffer.into())
}
