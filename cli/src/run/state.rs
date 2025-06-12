use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Instant;
use std::cell::RefCell;
use std::rc::Rc;
use sourcemap::SourceMap;

use crate::run::inspector;
use crate::run::capabilities::CapabilityRegistry;
use crate::manifest::myco_local::MycoLocalToml;

#[derive(Debug, Clone)]
pub struct DebugOptions {
    pub port: u16,
    pub break_on_start: bool,
    pub wait_for_connection: bool,
}

// Timer structure to track pending timeouts
pub struct Timer {
    pub id: u32,
    pub callback: v8::Global<v8::Function>,
    pub execute_at: Instant,
}

impl Timer {
    pub fn new(id: u32, callback: v8::Global<v8::Function>, execute_at: Instant) -> Self {
        Self {
            id,
            callback,
            execute_at,
        }
    }
}

// State that gets stored in the V8 isolate
pub struct MycoState {
    pub capabilities: CapabilityRegistry,
    pub module_cache: HashMap<String, v8::Global<v8::Module>>,
    pub timers: Vec<Timer>,
    pub next_timer_id: u32,
    pub module_url_to_path: HashMap<String, PathBuf>,
    pub source_maps: HashMap<String, SourceMap>,
    pub inspector: Option<Rc<RefCell<inspector::MycoInspector>>>,
    pub myco_local: Option<MycoLocalToml>,
}

impl MycoState {
    pub fn new(myco_local: Option<MycoLocalToml>) -> Self {
        Self {
            capabilities: CapabilityRegistry::new(),
            module_cache: HashMap::new(),
            timers: Vec::new(),
            next_timer_id: 1,
            module_url_to_path: HashMap::new(),
            source_maps: HashMap::new(),
            inspector: None,
            myco_local,
        }
    }
} 