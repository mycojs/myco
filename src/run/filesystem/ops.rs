use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use anyhow::anyhow;
use deno_core::{op, OpState};

use crate::{AnyError, Capability, create_token, Token};

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

fn canonical(dir: String, path: String) -> Result<PathBuf, AnyError> {
    let dir = PathBuf::from(dir).canonicalize()?;
    let path = dir.join(path).canonicalize()?;
    if !path.starts_with(&dir) {
        Err(anyhow!("Attempted to access a path outside of the token's scope"))
    } else {
        Ok(path)
    }
}

fn read_path(state: Rc<RefCell<OpState>>, token: Token, path: Option<String>) -> Result<PathBuf, AnyError> {
    if let Some(path) = path {
        let dir = match_capability!(state, token, ReadDir)?;
        canonical(dir, path)
    } else {
        Ok(PathBuf::from(match_capability!(state, token, ReadFile)?))
    }
}

#[op]
pub async fn myco_op_read_file(state: Rc<RefCell<OpState>>, token: Token, path: Option<String>) -> Result<String, AnyError> {
    let path = read_path(state, token, path)?;
    let contents = tokio::fs::read_to_string(path).await?;
    Ok(contents)
}

#[op]
pub fn myco_op_read_file_sync(state: Rc<RefCell<OpState>>, token: Token, path: Option<String>) -> Result<String, AnyError> {
    let path = read_path(state, token, path)?;
    let contents = std::fs::read_to_string(path)?;
    Ok(contents)
}

fn write_path(state: Rc<RefCell<OpState>>, token: Token, path: Option<String>) -> Result<PathBuf, AnyError> {
    if let Some(path) = path {
        let dir = match_capability!(state, token, WriteDir)?;
        canonical(dir, path)
    } else {
        Ok(PathBuf::from(match_capability!(state, token, WriteFile)?))
    }
}

#[op]
pub async fn myco_op_write_file(state: Rc<RefCell<OpState>>, token: Token, contents: String, path: Option<String>) -> Result<(), AnyError> {
    let path = write_path(state, token, path)?;
    tokio::fs::write(path, contents).await?;
    Ok(())
}

#[op]
pub fn myco_op_write_file_sync(state: Rc<RefCell<OpState>>, token: Token, contents: String, path: Option<String>) -> Result<(), AnyError> {
    let path = write_path(state, token, path)?;
    std::fs::write(path, contents)?;
    Ok(())
}

#[op]
pub async fn myco_op_remove_file(state: Rc<RefCell<OpState>>, token: Token, path: Option<String>) -> Result<(), AnyError> {
    let path = write_path(state, token, path)?;
    tokio::fs::remove_file(path).await?;
    Ok(())
}

#[op]
pub fn myco_op_remove_file_sync(state: Rc<RefCell<OpState>>, token: Token, path: Option<String>) -> Result<(), AnyError> {
    let path = write_path(state, token, path)?;
    std::fs::remove_file(path)?;
    Ok(())
}
