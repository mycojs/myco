use std::cell::RefCell;
use std::rc::Rc;

use deno_core::{op, OpState, ZeroCopyBuf};

use crate::{AnyError, Capability, create_token, Token};

#[op]
pub async fn myco_op_request_fetch_url(state: Rc<RefCell<OpState>>, url: String) -> Result<Token, AnyError> {
    Ok(create_token(state, Capability::FetchUrl(url)))
}

#[op]
pub async fn myco_op_request_fetch_prefix(state: Rc<RefCell<OpState>>, prefix: String) -> Result<Token, AnyError> {
    Ok(create_token(state, Capability::FetchPrefix(prefix)))
}

#[op]
async fn myco_op_fetch_url(state: Rc<RefCell<OpState>>, token: Token) -> Result<ZeroCopyBuf, AnyError> {
    let url = match_capability!(state, token, FetchUrl)?;
    let body = reqwest::get(url).await?.bytes().await?;
    let body = body.to_vec();
    Ok(body.into())
}
