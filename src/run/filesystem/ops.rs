use std::cell::RefCell;
use std::ops::Deref;
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
    let path = dir.join(path);
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

#[derive(serde::Serialize)]
pub struct Stats {
    pub is_file: bool,
    pub is_dir: bool,
    pub is_symlink: bool,
    pub size: u64,
    pub readonly: bool,
    pub modified: Option<u64>,
    pub accessed: Option<u64>,
    pub created: Option<u64>,
}

fn system_time_to_unix_time(t: Option<std::time::SystemTime>) -> Option<u64> {
    Some(t?.duration_since(std::time::UNIX_EPOCH).ok()?.as_secs())
}

impl Stats {
    fn from_metadata(metadata: std::fs::Metadata) -> Self {
        Self {
            is_file: metadata.is_file(),
            is_dir: metadata.is_dir(),
            is_symlink: metadata.file_type().is_symlink(),
            size: metadata.len(),
            readonly: metadata.permissions().readonly(),
            modified: system_time_to_unix_time(metadata.modified().ok()),
            accessed: system_time_to_unix_time(metadata.accessed().ok()),
            created: system_time_to_unix_time(metadata.created().ok()),
        }
    }
}

#[op]
pub async fn myco_op_stat_file(state: Rc<RefCell<OpState>>, token: Token, path: Option<String>) -> Result<Option<Stats>, AnyError> {
    if let Some(path) = read_path(state, token, path).ok() {
        let metadata = tokio::fs::metadata(path).await.ok();
        Ok(metadata.map(Stats::from_metadata))
    } else {
        Ok(None)
    }
}

#[op]
pub fn myco_op_stat_file_sync(state: Rc<RefCell<OpState>>, token: Token, path: Option<String>) -> Result<Option<Stats>, AnyError> {
    if let Some(path) = read_path(state, token, path).ok() {
        let metadata = std::fs::metadata(path).ok();
        Ok(metadata.map(Stats::from_metadata))
    } else {
        Ok(None)
    }
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

#[op]
pub async fn myco_op_mkdirp(state: Rc<RefCell<OpState>>, token: Token, path: String) -> Result<(), AnyError> {
    let path = write_path(state, token, Some(path))?;
    tokio::fs::create_dir_all(path).await?;
    Ok(())
}

#[op]
pub fn myco_op_mkdirp_sync(state: Rc<RefCell<OpState>>, token: Token, path: String) -> Result<(), AnyError> {
    let path = write_path(state, token, Some(path))?;
    std::fs::create_dir_all(path)?;
    Ok(())
}
