use std::cell::RefCell;
use std::rc::Rc;

use deno_core::{BufMutView, BufView, op, OpState, ZeroCopyBuf};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::{AnyError, Capability, create_token, invalidate_token, Token};

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

#[op]
pub async fn myco_op_bind_tcp_listener(state: Rc<RefCell<OpState>>, addr: String) -> Result<Token, AnyError> {
    let listener = tokio::net::TcpListener::bind(addr).await?;
    let listener = Box::new(RefCell::new(listener));
    Ok(create_token(state, Capability::TcpListener(listener)))
}

#[op]
pub async fn myco_op_accept_tcp_stream(state: Rc<RefCell<OpState>>, token: Token) -> Result<Token, AnyError> {
    let stream = {
        let state = state.borrow();
        let listener = match_capability_refcell_mut!(state, token, TcpListener)?;
        let (stream, _) = listener.accept().await?;
        Box::new(RefCell::new(stream))
    };
    Ok(create_token(state, Capability::TcpStream(stream)))
}

#[op]
pub async fn myco_op_read_all_tcp_stream(state: Rc<RefCell<OpState>>, token: Token) -> Result<ZeroCopyBuf, AnyError> {
    let state = state.borrow();
    let mut stream = match_capability_refcell_mut!(state, token, TcpStream)?;
    let mut buf = Vec::new();
    stream.read_to_end(&mut buf).await?;
    Ok(buf.into())
}

#[op]
pub async fn myco_op_write_all_tcp_stream(state: Rc<RefCell<OpState>>, token: Token, buf: ZeroCopyBuf) -> Result<(), AnyError> {
    let state = state.borrow();
    let mut stream = match_capability_refcell_mut!(state, token, TcpStream)?;
    let buf = BufView::from(buf);
    stream.write_all(buf.as_ref()).await?;
    Ok(())
}

#[op]
pub async fn myco_op_close_tcp_stream(state: Rc<RefCell<OpState>>, token: Token) -> Result<(), AnyError> {
    let state = state.borrow();
    let mut stream = match_capability_refcell_mut!(state, token, TcpStream)?;
    stream.shutdown().await?;
    Ok(())
}

#[op]
pub async fn myco_op_close_tcp_listener(state: Rc<RefCell<OpState>>, token: Token) -> Result<(), AnyError> {
    invalidate_token(state, token);
    Ok(())
}
