use log::{debug, trace};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use rand::{thread_rng, Rng};

use crate::run::state::MycoState;

pub type Token = String;

pub fn create_token(state: Rc<RefCell<MycoState>>, capability: Capability) -> Token {
    debug!("Creating capability token for: {:?}", capability);
    let mut state = state.borrow_mut();
    let registry = &mut state.capabilities;
    let token = registry.register(capability);
    trace!("Generated capability token: {}", token);
    token
}

pub fn invalidate_token(state: Rc<RefCell<MycoState>>, token: Token) -> Option<Capability> {
    debug!("Invalidating capability token: {}", token);
    let mut state = state.borrow_mut();
    let registry = &mut state.capabilities;
    let capability = registry.unregister(token.clone());
    if capability.is_some() {
        trace!("Successfully invalidated token: {}", token);
    } else {
        trace!("Token not found for invalidation: {}", token);
    }
    capability
}

#[derive(Debug)]
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

impl Default for CapabilityRegistry {
    fn default() -> Self {
        Self::new()
    }
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
        debug!(
            "Registered capability with token, total capabilities: {}",
            self.capabilities.len()
        );
        token
    }

    pub fn unregister(&mut self, token: String) -> Option<Capability> {
        let capability = self.capabilities.remove(&token);
        if capability.is_some() {
            debug!(
                "Unregistered capability, remaining capabilities: {}",
                self.capabilities.len()
            );
        }
        capability
    }

    pub fn get(&self, name: &str) -> Option<&Capability> {
        trace!("Looking up capability with token: {}", name);
        self.capabilities.get(name)
    }
}

#[macro_export]
macro_rules! match_capability {
    ($state:expr, $token:ident, $capability:ident) => {{
        let state = $state.borrow();
        let registry = &state.capabilities;
        match registry.get(&$token) {
            Some($crate::Capability::$capability(value)) => Ok(value.clone()),
            _ => Err($crate::errors::MycoError::Internal {
                message: "Invalid token".to_string(),
            }),
        }
    }};
}

#[macro_export]
macro_rules! match_capability_refcell_mut {
    ($state:expr, $token:ident, $capability:ident) => {{
        let state = $state.borrow();
        let registry = &state.capabilities;
        match registry.get(&$token) {
            Some($crate::Capability::$capability(value)) => Ok(value.borrow_mut()),
            _ => Err($crate::errors::MycoError::Internal {
                message: "Invalid token".to_string(),
            }),
        }
    }};
}

#[macro_export]
macro_rules! match_capability_refcell {
    ($state:expr, $token:ident, $capability:ident) => {{
        let state = $state.borrow();
        let registry = &state.capabilities;
        match registry.get(&$token) {
            Some($crate::Capability::$capability(value)) => Ok(value.clone()),
            _ => Err($crate::errors::MycoError::Internal {
                message: "Invalid token".to_string(),
            }),
        }
    }};
}
