use std::cell::RefCell;
use std::rc::Rc;

use deno_core::{op, OpState};

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
async fn myco_op_fetch_url(state: Rc<RefCell<OpState>>, token: Token) -> Result<String, AnyError> {
    let url = match_capability!(state, token, FetchUrl)?;
    let body = reqwest::get(url).await?.text().await?;
    Ok(body)
}
