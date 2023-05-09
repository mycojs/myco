use std::collections::HashMap;
use rand::{Rng, thread_rng};

pub enum Capability {
    ReadFile(String),
    WriteFile(String),
    ReadDir(String),
    WriteDir(String),
    FetchUrl(String),
    FetchPrefix(String),
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

    pub fn get(&self, name: &str) -> Option<&Capability> {
        self.capabilities.get(name)
    }
}
