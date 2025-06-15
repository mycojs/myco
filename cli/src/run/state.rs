use crate::manifest::myco_local::MycoLocalToml;
use crate::run::capabilities::CapabilityRegistry;
use crate::run::inspector;
use crate::Capability;
use sourcemap::SourceMap;
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Instant;
use tokio::sync::mpsc;

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

    // Async operation management
    pub runtime_handle: tokio::runtime::Handle,
    pub pending_ops: HashMap<u32, v8::Global<v8::PromiseResolver>>,
    pub next_op_id: u32,
    pub op_sender: mpsc::UnboundedSender<FinalOpResult>,
    pub op_receiver: Option<mpsc::UnboundedReceiver<FinalOpResult>>,
}

impl MycoState {
    pub fn new(myco_local: Option<MycoLocalToml>, runtime_handle: tokio::runtime::Handle) -> Self {
        let (op_sender, op_receiver) = mpsc::unbounded_channel();

        Self {
            capabilities: CapabilityRegistry::new(),
            module_cache: HashMap::new(),
            timers: Vec::new(),
            next_timer_id: 1,
            module_url_to_path: HashMap::new(),
            source_maps: HashMap::new(),
            inspector: None,
            myco_local,
            runtime_handle,
            pending_ops: HashMap::new(),
            next_op_id: 1,
            op_sender,
            op_receiver: Some(op_receiver),
        }
    }

    pub fn get_next_op_id(&mut self) -> u32 {
        let id = self.next_op_id;
        self.next_op_id += 1;
        id
    }

    pub fn register_pending_op(&mut self, op_id: u32, resolver: v8::Global<v8::PromiseResolver>) {
        self.pending_ops.insert(op_id, resolver);
    }

    pub fn complete_pending_op(&mut self, op_id: u32) -> Option<v8::Global<v8::PromiseResolver>> {
        self.pending_ops.remove(&op_id)
    }
}

pub enum OpResult {
    Void(Result<(), String>),
    Binary(Result<Vec<u8>, String>),
    Capability(Result<Capability, String>),
    Json(Result<String, String>),
}

impl OpResult {
    pub fn to_final_op_result(self, op_id: u32) -> FinalOpResult {
        match self {
            OpResult::Void(result) => FinalOpResult::Void { op_id, result },
            OpResult::Binary(result) => FinalOpResult::Binary { op_id, result },
            OpResult::Capability(result) => FinalOpResult::Capability { op_id, result },
            OpResult::Json(result) => FinalOpResult::Json { op_id, result },
        }
    }
}

#[derive(Debug)]
pub enum FinalOpResult {
    Void {
        op_id: u32,
        result: Result<(), String>,
    },
    Binary {
        op_id: u32,
        result: Result<Vec<u8>, String>,
    },
    Capability {
        op_id: u32,
        result: Result<Capability, String>,
    },
    Json {
        op_id: u32,
        result: Result<String, String>,
    },
}

impl FinalOpResult {
    pub fn resolve_promise(
        self,
        scope: &mut v8::HandleScope,
        resolver: v8::Local<v8::PromiseResolver>,
    ) {
        match self {
            FinalOpResult::Void { result, .. } => {
                resolve_void_result(scope, resolver, result);
            }
            FinalOpResult::Binary { result, .. } => {
                resolve_binary_result(scope, resolver, result);
            }
            FinalOpResult::Capability { result, .. } => {
                resolve_capability_result(scope, resolver, result);
            }
            FinalOpResult::Json { result, .. } => {
                resolve_json_result(scope, resolver, result);
            }
        }
    }

    pub fn get_op_id(&self) -> u32 {
        match self {
            FinalOpResult::Void { op_id, .. } => *op_id,
            FinalOpResult::Binary { op_id, .. } => *op_id,
            FinalOpResult::Capability { op_id, .. } => *op_id,
            FinalOpResult::Json { op_id, .. } => *op_id,
        }
    }
}

fn resolve_void_result(
    scope: &mut v8::HandleScope,
    resolver: v8::Local<v8::PromiseResolver>,
    result: Result<(), String>,
) {
    match result {
        Ok(_) => {
            let undefined_value = v8::undefined(scope).into();
            resolver.resolve(scope, undefined_value);
        }
        Err(e) => {
            let error = v8::String::new(scope, &e).unwrap();
            resolver.reject(scope, error.into());
        }
    }
}

fn resolve_binary_result(
    scope: &mut v8::HandleScope,
    resolver: v8::Local<v8::PromiseResolver>,
    result: Result<Vec<u8>, String>,
) {
    match result {
        Ok(data) => {
            let array_buffer = v8::ArrayBuffer::new(scope, data.len());
            let backing_store = array_buffer.get_backing_store();
            unsafe {
                let ptr = backing_store.data().unwrap().as_ptr() as *mut u8;
                std::ptr::copy_nonoverlapping(data.as_ptr(), ptr, data.len());
            }
            let uint8_array = v8::Uint8Array::new(scope, array_buffer, 0, data.len()).unwrap();
            resolver.resolve(scope, uint8_array.into());
        }
        Err(e) => {
            let error = v8::String::new(scope, &e).unwrap();
            resolver.reject(scope, error.into());
        }
    }
}

fn resolve_capability_result(
    scope: &mut v8::HandleScope,
    resolver: v8::Local<v8::PromiseResolver>,
    result: Result<Capability, String>,
) {
    match result {
        Ok(capability) => {
            let state_ptr = scope.get_data(0) as *mut MycoState;
            if !state_ptr.is_null() {
                let state = unsafe { &mut *state_ptr };
                let token = state.capabilities.register(capability);
                let token_string = v8::String::new(scope, &token).unwrap();
                resolver.resolve(scope, token_string.into());
            } else {
                let error = v8::String::new(scope, "Failed to get isolate state").unwrap();
                resolver.reject(scope, error.into());
            }
        }
        Err(e) => {
            let error = v8::String::new(scope, &e).unwrap();
            resolver.reject(scope, error.into());
        }
    }
}

fn resolve_json_result(
    scope: &mut v8::HandleScope,
    resolver: v8::Local<v8::PromiseResolver>,
    result: Result<String, String>,
) {
    match result {
        Ok(json_string) => {
            let json_value = v8::String::new(scope, &json_string).unwrap();
            let parsed = v8::json::parse(scope, json_value).unwrap();
            resolver.resolve(scope, parsed);
        }
        Err(e) => {
            let error = v8::String::new(scope, &e).unwrap();
            resolver.reject(scope, error.into());
        }
    }
}
