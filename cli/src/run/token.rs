use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use deno_core::{OpState};
use rand::{Rng, thread_rng};

pub type Token = String;

pub fn create_token(state: Rc<RefCell<OpState>>, capability: Capability) -> Token {
    let mut state = state.borrow_mut();
    let registry = state.borrow_mut::<CapabilityRegistry>();
    registry.register(capability)
}

pub fn invalidate_token(state: Rc<RefCell<OpState>>, token: Token) -> Option<Capability> {
    let mut state = state.borrow_mut();
    let registry = state.borrow_mut::<CapabilityRegistry>();
    registry.unregister(token)
}

pub enum Capability {
    ReadFile(String),
    WriteFile(String),
    ExecFile(String),
    ReadDir(String),
    WriteDir(String),
    ExecDir(String),
    FetchUrl(String),
    FetchPrefix(String),
    TcpListener(Box<RefCell<tokio::net::TcpListener>>),
    TcpStream(Box<RefCell<tokio::net::TcpStream>>),
}

pub struct CapabilityRegistry {
    capabilities: HashMap<String, Capability>,
}

impl CapabilityRegistry {
    pub fn new() -> Self {
        Self {
            capabilities: HashMap::new(),
        }
    }

    pub fn register(&mut self, capability: Capability) -> String {
        let token: String = thread_rng()
            .sample_iter(&rand::distributions::Alphanumeric)
            .take(30)
            .map(char::from)
            .collect();
        self.capabilities.insert(token.clone(), capability);
        token
    }

    pub fn unregister(&mut self, token: String) -> Option<Capability> {
        self.capabilities.remove(&token)
    }

    pub fn get(&self, name: &str) -> Option<&Capability> {
        self.capabilities.get(name)
    }
}

#[macro_export]
macro_rules! match_capability {
    ($state:expr, $token:ident, $capability:ident) => {
        {
            let state = $state.borrow();
            let registry = state.borrow::<crate::CapabilityRegistry>();
            match registry.get(&$token) {
                Some(crate::Capability::$capability(value)) => Ok(value.clone()),
                _ => Err(anyhow::anyhow!("Invalid token")),
            }
        }
    };
}

#[macro_export]
macro_rules! match_capability_refcell_mut {
    ($state:expr, $token:ident, $capability:ident) => {
        {
            let registry = $state.borrow::<crate::CapabilityRegistry>();
            match registry.get(&$token) {
                Some(crate::Capability::$capability(value)) => Ok(value.borrow_mut()),
                _ => Err(anyhow::anyhow!("Invalid token")),
            }
        }
    };
}

#[macro_export]
macro_rules! match_capability_refcell {
    ($state:expr, $token:ident, $capability:ident) => {
        {
            let registry = $state.borrow::<crate::CapabilityRegistry>();
            match registry.get(&$token) {
                Some(crate::Capability::$capability(value)) => Ok(value.clone()),
                _ => Err(anyhow::anyhow!("Invalid token")),
            }
        }
    };
}