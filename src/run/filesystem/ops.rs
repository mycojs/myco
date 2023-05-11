use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use anyhow::anyhow;
use deno_core::{op, OpState};

use crate::{AnyError, Capability, CapabilityRegistry, create_token, Token};

#[op]
pub async fn myco_op_request_read_file(state: Rc<RefCell<OpState>>, path: String) -> Result<Token, AnyError> {
    Ok(create_token(state, Capability::ReadFile(path)))
}

#[op]
pub async fn myco_op_request_write_file(state: Rc<RefCell<OpState>>, path: String) -> Result<Token, AnyError> {
    Ok(create_token(state, Capability::WriteFile(path)))
}

#[op]
pub async fn myco_op_request_read_dir(state: Rc<RefCell<OpState>>, path: String) -> Result<Token, AnyError> {
    let path_buf = PathBuf::from(path.clone());
    if !path_buf.exists() {
        tokio::fs::create_dir_all(&path_buf).await?;
    }
    Ok(create_token(state, Capability::ReadDir(path)))
}

#[op]
pub async fn myco_op_request_write_dir(state: Rc<RefCell<OpState>>, path: String) -> Result<Token, AnyError> {
    let path_buf = PathBuf::from(path.clone());
    if !path_buf.exists() {
        tokio::fs::create_dir_all(&path_buf).await?;
    }
    Ok(create_token(state, Capability::WriteDir(path)))
}

#[op]
pub async fn myco_op_read_file(state: Rc<RefCell<OpState>>, token: Token) -> Result<String, AnyError> {
    let state = state.borrow();
    let registry = state.borrow::<CapabilityRegistry>();
    let path = match registry.get(&token) {
        Some(Capability::ReadFile(path)) => path,
        _ => return Err(anyhow!("Invalid token")),
    };
    let contents = tokio::fs::read_to_string(path).await?;
    Ok(contents)
}

#[op]
pub async fn myco_op_write_file(state: Rc<RefCell<OpState>>, token: Token, contents: String) -> Result<(), AnyError> {
    let state = state.borrow();
    let registry = state.borrow::<CapabilityRegistry>();
    let path = match registry.get(&token) {
        Some(Capability::WriteFile(path)) => path,
        _ => return Err(anyhow!("Invalid token")),
    };
    tokio::fs::write(path, contents).await?;
    Ok(())
}

#[op]
pub async fn myco_op_remove_file(state: Rc<RefCell<OpState>>, token: Token) -> Result<(), AnyError> {
    let state = state.borrow();
    let registry = state.borrow::<CapabilityRegistry>();
    let path = match registry.get(&token) {
        Some(Capability::WriteFile(path)) => path,
        _ => return Err(anyhow!("Invalid token")),
    };
    tokio::fs::remove_file(path).await?;
    Ok(())
}

#[op]
pub async fn myco_op_read_file_in_dir(state: Rc<RefCell<OpState>>, token: Token, path: String) -> Result<String, AnyError> {
    let state = state.borrow();
    let registry = state.borrow::<CapabilityRegistry>();
    let dir = match registry.get(&token) {
        Some(Capability::ReadDir(dir)) => dir,
        _ => return Err(anyhow!("Invalid token")),
    };
    let dir = PathBuf::from(dir).canonicalize()?;
    let path = dir.join(path).canonicalize()?;
    if !path.starts_with(&dir) {
        return Err(anyhow!("Attempted to access a path outside of the token's scope"));
    }
    let contents = tokio::fs::read_to_string(path).await?;
    Ok(contents)
}

#[op]
pub async fn myco_op_write_file_in_dir(state: Rc<RefCell<OpState>>, token: Token, path: String, contents: String) -> Result<(), AnyError> {
    let state = state.borrow();
    let registry = state.borrow::<CapabilityRegistry>();
    let dir = match registry.get(&token) {
        Some(Capability::WriteDir(dir)) => dir,
        _ => return Err(anyhow!("Invalid token")),
    };
    let dir = PathBuf::from(dir).canonicalize()?;
    let path = dir.join(path).canonicalize()?;
    if !path.starts_with(&dir) {
        return Err(anyhow!("Attempted to access a path outside of the token's scope"));
    }
    tokio::fs::write(path, contents).await?;
    Ok(())
}

#[op]
pub async fn myco_op_remove_file_in_dir(state: Rc<RefCell<OpState>>, token: Token, path: String) -> Result<(), AnyError> {
    let state = state.borrow();
    let registry = state.borrow::<CapabilityRegistry>();
    let dir = match registry.get(&token) {
        Some(Capability::WriteDir(dir)) => dir,
        _ => return Err(anyhow!("Invalid token")),
    };
    let dir = PathBuf::from(dir).canonicalize()?;
    let path = dir.join(path).canonicalize()?;
    if !path.starts_with(&dir) {
        return Err(anyhow!("Attempted to access a path outside of the token's scope"));
    }
    tokio::fs::remove_file(path).await?;
    Ok(())
}
