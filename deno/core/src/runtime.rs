// Copyright 2018-2023 the Deno authors. All rights reserved. MIT license.

use crate::bindings;
use crate::error::generic_error;
use crate::error::to_v8_type_error;
use crate::error::JsError;
use crate::extensions::OpDecl;
use crate::extensions::OpEventLoopFn;
use crate::inspector::JsRuntimeInspector;
use crate::module_specifier::ModuleSpecifier;
use crate::modules::ExtModuleLoaderCb;
use crate::modules::ModuleCode;
use crate::modules::ModuleError;
use crate::modules::ModuleId;
use crate::modules::ModuleLoadId;
use crate::modules::ModuleLoader;
use crate::modules::ModuleMap;
use crate::ops::*;
use crate::realm::ContextState;
use crate::realm::JsRealm;
use crate::realm::JsRealmLocal;
use crate::snapshot_util;
use crate::source_map::SourceMapCache;
use crate::source_map::SourceMapGetter;
use crate::Extension;
use crate::ExtensionFileSource;
use crate::NoopModuleLoader;
use crate::OpMiddlewareFn;
use crate::OpResult;
use crate::OpState;
use crate::PromiseId;
use anyhow::Context as AnyhowContext;
use anyhow::Error;
use futures::channel::oneshot;
use futures::future::poll_fn;
use futures::future::Future;
use futures::future::FutureExt;
use futures::future::MaybeDone;
use futures::stream::FuturesUnordered;
use futures::stream::StreamExt;
use futures::task::noop_waker;
use futures::task::AtomicWaker;
use smallvec::SmallVec;
use std::any::Any;
use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::c_void;
use std::option::Option;
use std::pin::Pin;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::Once;
use std::task::Context;
use std::task::Poll;
use v8::OwnedIsolate;

pub enum Snapshot {
  Static(&'static [u8]),
  JustCreated(v8::StartupData),
  Boxed(Box<[u8]>),
}

pub type JsErrorCreateFn = dyn Fn(JsError) -> Error;

pub type GetErrorClassFn = &'static dyn for<'e> Fn(&'e Error) -> &'static str;

/// Objects that need to live as long as the isolate
#[derive(Default)]
struct IsolateAllocations {
  near_heap_limit_callback_data:
    Option<(Box<RefCell<dyn Any>>, v8::NearHeapLimitCallback)>,
}

/// A single execution context of JavaScript. Corresponds roughly to the "Web
/// Worker" concept in the DOM. A JsRuntime is a Future that can be used with
/// an event loop (Tokio, async_std).
////
/// The JsRuntime future completes when there is an error or when all
/// pending ops have completed.
///
/// Pending ops are created in JavaScript by calling Deno.core.opAsync(), and in Rust
/// by implementing an async function that takes a serde::Deserialize "control argument"
/// and an optional zero copy buffer, each async Op is tied to a Promise in JavaScript.
pub struct JsRuntime {
  state: Rc<RefCell<JsRuntimeState>>,
  module_map: Option<Rc<RefCell<ModuleMap>>>,
  // This is an Option<OwnedIsolate> instead of just OwnedIsolate to workaround
  // a safety issue with SnapshotCreator. See JsRuntime::drop.
  v8_isolate: Option<v8::OwnedIsolate>,
  snapshot_options: snapshot_util::SnapshotOptions,
  allocations: IsolateAllocations,
  extensions: Rc<RefCell<Vec<Extension>>>,
  event_loop_middlewares: Vec<Box<OpEventLoopFn>>,
  // Marks if this is considered the top-level runtime. Used only be inspector.
  is_main: bool,
}

pub(crate) struct DynImportModEvaluate {
  load_id: ModuleLoadId,
  module_id: ModuleId,
  promise: v8::Global<v8::Promise>,
  module: v8::Global<v8::Module>,
}

pub(crate) struct ModEvaluate {
  pub(crate) promise: Option<v8::Global<v8::Promise>>,
  pub(crate) has_evaluated: bool,
  pub(crate) handled_promise_rejections: Vec<v8::Global<v8::Promise>>,
  sender: oneshot::Sender<Result<(), Error>>,
}

pub struct CrossIsolateStore<T>(Arc<Mutex<CrossIsolateStoreInner<T>>>);

struct CrossIsolateStoreInner<T> {
  map: HashMap<u32, T>,
  last_id: u32,
}

impl<T> CrossIsolateStore<T> {
  pub(crate) fn insert(&self, value: T) -> u32 {
    let mut store = self.0.lock().unwrap();
    let last_id = store.last_id;
    store.map.insert(last_id, value);
    store.last_id += 1;
    last_id
  }

  pub(crate) fn take(&self, id: u32) -> Option<T> {
    let mut store = self.0.lock().unwrap();
    store.map.remove(&id)
  }
}

impl<T> Default for CrossIsolateStore<T> {
  fn default() -> Self {
    CrossIsolateStore(Arc::new(Mutex::new(CrossIsolateStoreInner {
      map: Default::default(),
      last_id: 0,
    })))
  }
}

impl<T> Clone for CrossIsolateStore<T> {
  fn clone(&self) -> Self {
    Self(self.0.clone())
  }
}

pub type SharedArrayBufferStore =
  CrossIsolateStore<v8::SharedRef<v8::BackingStore>>;

pub type CompiledWasmModuleStore = CrossIsolateStore<v8::CompiledWasmModule>;

/// Internal state for JsRuntime which is stored in one of v8::Isolate's
/// embedder slots.
pub struct JsRuntimeState {
  global_realm: Option<JsRealm>,
  known_realms: Vec<v8::Weak<v8::Context>>,
  pub(crate) has_tick_scheduled: bool,
  pub(crate) pending_dyn_mod_evaluate: Vec<DynImportModEvaluate>,
  pub(crate) pending_mod_evaluate: Option<ModEvaluate>,
  /// A counter used to delay our dynamic import deadlock detection by one spin
  /// of the event loop.
  dyn_module_evaluate_idle_counter: u32,
  pub(crate) source_map_getter: Option<Rc<Box<dyn SourceMapGetter>>>,
  pub(crate) source_map_cache: Rc<RefCell<SourceMapCache>>,
  pub(crate) pending_ops: FuturesUnordered<OpCall>,
  pub(crate) have_unpolled_ops: bool,
  pub(crate) op_state: Rc<RefCell<OpState>>,
  pub(crate) shared_array_buffer_store: Option<SharedArrayBufferStore>,
  pub(crate) compiled_wasm_module_store: Option<CompiledWasmModuleStore>,
  /// The error that was passed to an `op_dispatch_exception` call.
  /// It will be retrieved by `exception_to_err_result` and used as an error
  /// instead of any other exceptions.
  // TODO(nayeemrmn): This is polled in `exception_to_err_result()` which is
  // flimsy. Try to poll it similarly to `pending_promise_rejections`.
  pub(crate) dispatched_exception: Option<v8::Global<v8::Value>>,
  pub(crate) inspector: Option<Rc<RefCell<JsRuntimeInspector>>>,
  waker: AtomicWaker,
}

fn v8_init(
  v8_platform: Option<v8::SharedRef<v8::Platform>>,
  predictable: bool,
) {
  // Include 10MB ICU data file.
  #[repr(C, align(16))]
  struct IcuData([u8; 10541264]);
  static ICU_DATA: IcuData = IcuData(*include_bytes!("icudtl.dat"));
  v8::icu::set_common_data_72(&ICU_DATA.0).unwrap();

  let flags = concat!(
    " --wasm-test-streaming",
    " --harmony-import-assertions",
    " --no-validate-asm",
    " --turbo_fast_api_calls",
    " --harmony-change-array-by-copy",
  );

  if predictable {
    v8::V8::set_flags_from_string(&format!(
      "{}{}",
      flags, " --predictable --random-seed=42"
    ));
  } else {
    v8::V8::set_flags_from_string(flags);
  }

  let v8_platform = v8_platform
    .unwrap_or_else(|| v8::new_default_platform(0, false).make_shared());
  v8::V8::initialize_platform(v8_platform);
  v8::V8::initialize();
}

pub const V8_WRAPPER_TYPE_INDEX: i32 = 0;
pub const V8_WRAPPER_OBJECT_INDEX: i32 = 1;

#[derive(Default)]
pub struct RuntimeOptions {
  /// Source map reference for errors.
  pub source_map_getter: Option<Box<dyn SourceMapGetter>>,

  /// Allows to map error type to a string "class" used to represent
  /// error in JavaScript.
  pub get_error_class_fn: Option<GetErrorClassFn>,

  /// Implementation of `ModuleLoader` which will be
  /// called when V8 requests to load ES modules.
  ///
  /// If not provided runtime will error if code being
  /// executed tries to load modules.
  pub module_loader: Option<Rc<dyn ModuleLoader>>,

  /// JsRuntime extensions, not to be confused with ES modules.
  /// Only ops registered by extensions will be initialized. If you need
  /// to execute JS code from extensions, pass source files in `js` or `esm`
  /// option on `ExtensionBuilder`.
  ///
  /// If you are creating a runtime from a snapshot take care not to include
  /// JavaScript sources in the extensions.
  pub extensions: Vec<Extension>,

  /// V8 snapshot that should be loaded on startup.
  pub startup_snapshot: Option<Snapshot>,

  /// Prepare runtime to take snapshot of loaded code.
  /// The snapshot is deterministic and uses predictable random numbers.
  pub will_snapshot: bool,

  /// An optional callback that will be called for each module that is loaded
  /// during snapshotting. This callback can be used to transpile source on the
  /// fly, during snapshotting, eg. to transpile TypeScript to JavaScript.
  pub snapshot_module_load_cb: Option<ExtModuleLoaderCb>,

  /// Isolate creation parameters.
  pub create_params: Option<v8::CreateParams>,

  /// V8 platform instance to use. Used when Deno initializes V8
  /// (which it only does once), otherwise it's silenty dropped.
  pub v8_platform: Option<v8::SharedRef<v8::Platform>>,

  /// The store to use for transferring SharedArrayBuffers between isolates.
  /// If multiple isolates should have the possibility of sharing
  /// SharedArrayBuffers, they should use the same [SharedArrayBufferStore]. If
  /// no [SharedArrayBufferStore] is specified, SharedArrayBuffer can not be
  /// serialized.
  pub shared_array_buffer_store: Option<SharedArrayBufferStore>,

  /// The store to use for transferring `WebAssembly.Module` objects between
  /// isolates.
  /// If multiple isolates should have the possibility of sharing
  /// `WebAssembly.Module` objects, they should use the same
  /// [CompiledWasmModuleStore]. If no [CompiledWasmModuleStore] is specified,
  /// `WebAssembly.Module` objects cannot be serialized.
  pub compiled_wasm_module_store: Option<CompiledWasmModuleStore>,

  /// Start inspector instance to allow debuggers to connect.
  pub inspector: bool,

  /// Describe if this is the main runtime instance, used by debuggers in some
  /// situation - like disconnecting when program finishes running.
  pub is_main: bool,
}

impl Drop for JsRuntime {
  fn drop(&mut self) {
    if let Some(v8_isolate) = self.v8_isolate.as_mut() {
      Self::drop_state_and_module_map(v8_isolate);
    }
  }
}

impl JsRuntime {
  const STATE_DATA_OFFSET: u32 = 0;
  const MODULE_MAP_DATA_OFFSET: u32 = 1;

  /// Only constructor, configuration is done through `options`.
  pub fn new(mut options: RuntimeOptions) -> Self {
    let v8_platform = options.v8_platform.take();

    static DENO_INIT: Once = Once::new();
    DENO_INIT.call_once(move || v8_init(v8_platform, options.will_snapshot));

    // Add builtins extension
    // TODO(bartlomieju): remove this in favor of `SnapshotOptions`.
    let has_startup_snapshot = options.startup_snapshot.is_some();
    if !has_startup_snapshot {
      options
        .extensions
        .insert(0, crate::ops_builtin::core::init_ops_and_esm());
    } else {
      options
        .extensions
        .insert(0, crate::ops_builtin::core::init_ops());
    }

    let ops = Self::collect_ops(&mut options.extensions);
    let mut op_state = OpState::new();

    if let Some(get_error_class_fn) = options.get_error_class_fn {
      op_state.get_error_class_fn = get_error_class_fn;
    }
    let op_state = Rc::new(RefCell::new(op_state));

    let align = std::mem::align_of::<usize>();
    let layout = std::alloc::Layout::from_size_align(
      std::mem::size_of::<*mut v8::OwnedIsolate>(),
      align,
    )
    .unwrap();
    assert!(layout.size() > 0);
    let isolate_ptr: *mut v8::OwnedIsolate =
      // SAFETY: we just asserted that layout has non-0 size.
      unsafe { std::alloc::alloc(layout) as *mut _ };

    let state_rc = Rc::new(RefCell::new(JsRuntimeState {
      pending_dyn_mod_evaluate: vec![],
      pending_mod_evaluate: None,
      dyn_module_evaluate_idle_counter: 0,
      has_tick_scheduled: false,
      source_map_getter: options.source_map_getter.map(Rc::new),
      source_map_cache: Default::default(),
      pending_ops: FuturesUnordered::new(),
      shared_array_buffer_store: options.shared_array_buffer_store,
      compiled_wasm_module_store: options.compiled_wasm_module_store,
      op_state: op_state.clone(),
      waker: AtomicWaker::new(),
      have_unpolled_ops: false,
      dispatched_exception: None,
      // Some fields are initialized later after isolate is created
      inspector: None,
      global_realm: None,
      known_realms: Vec::with_capacity(1),
    }));

    let weak = Rc::downgrade(&state_rc);
    let op_ctxs = ops
      .into_iter()
      .enumerate()
      .map(|(id, decl)| {
        OpCtx::new(id as u16, 0, Rc::new(decl), op_state.clone(), weak.clone())
      })
      .collect::<Vec<_>>()
      .into_boxed_slice();

    let snapshot_options = snapshot_util::SnapshotOptions::from_bools(
      options.startup_snapshot.is_some(),
      options.will_snapshot,
    );
    let refs = bindings::external_references(&op_ctxs);
    // V8 takes ownership of external_references.
    let refs: &'static v8::ExternalReferences = Box::leak(Box::new(refs));
    let global_context;
    let mut maybe_snapshotted_data = None;

    let mut isolate = if snapshot_options.will_snapshot() {
      let snapshot_creator =
        snapshot_util::create_snapshot_creator(refs, options.startup_snapshot);
      let mut isolate = JsRuntime::setup_isolate(snapshot_creator);
      {
        let scope = &mut v8::HandleScope::new(&mut isolate);
        let context =
          bindings::initialize_context(scope, &op_ctxs, snapshot_options);

        // Get module map data from the snapshot
        if has_startup_snapshot {
          maybe_snapshotted_data =
            Some(snapshot_util::get_snapshotted_data(scope, context));
        }

        global_context = v8::Global::new(scope, context);
      }
      isolate
    } else {
      #[allow(unused_mut)]
      let mut params = options
        .create_params
        .take()
        .unwrap_or_else(|| {
          v8::CreateParams::default().embedder_wrapper_type_info_offsets(
            V8_WRAPPER_TYPE_INDEX,
            V8_WRAPPER_OBJECT_INDEX,
          )
        })
        .external_references(&**refs);

      if let Some(snapshot) = options.startup_snapshot {
        params = match snapshot {
          Snapshot::Static(data) => params.snapshot_blob(data),
          Snapshot::JustCreated(data) => params.snapshot_blob(data),
          Snapshot::Boxed(data) => params.snapshot_blob(data),
        };
      }

      let isolate = v8::Isolate::new(params);
      let mut isolate = JsRuntime::setup_isolate(isolate);
      {
        let scope = &mut v8::HandleScope::new(&mut isolate);
        let context =
          bindings::initialize_context(scope, &op_ctxs, snapshot_options);

        // Get module map data from the snapshot
        if has_startup_snapshot {
          maybe_snapshotted_data =
            Some(snapshot_util::get_snapshotted_data(scope, context));
        }

        global_context = v8::Global::new(scope, context);
      }

      isolate
    };

    // SAFETY: this is first use of `isolate_ptr` so we are sure we're
    // not overwriting an existing pointer.
    isolate = unsafe {
      isolate_ptr.write(isolate);
      isolate_ptr.read()
    };

    global_context.open(&mut isolate).set_slot(
      &mut isolate,
      Rc::new(RefCell::new(ContextState {
        op_ctxs,
        ..Default::default()
      })),
    );

    op_state.borrow_mut().put(isolate_ptr);
    let inspector = if options.inspector {
      Some(JsRuntimeInspector::new(
        &mut isolate,
        global_context.clone(),
        options.is_main,
      ))
    } else {
      None
    };

    let loader = options
      .module_loader
      .unwrap_or_else(|| Rc::new(NoopModuleLoader));
    let num_extensions = options.extensions.len();
    let extensions = Rc::new(RefCell::new(options.extensions));
    let ext_loader = Rc::new(crate::modules::ExtModuleLoader::new(
      Some(loader.clone()),
      extensions.clone(),
      options.snapshot_module_load_cb,
    ));

    {
      let mut state = state_rc.borrow_mut();
      state.global_realm = Some(JsRealm::new(global_context.clone()));
      state.inspector = inspector;
      state
        .known_realms
        .push(v8::Weak::new(&mut isolate, &global_context));
    }
    isolate.set_data(
      Self::STATE_DATA_OFFSET,
      Rc::into_raw(state_rc.clone()) as *mut c_void,
    );
    let module_map_rc =
      Rc::new(RefCell::new(ModuleMap::new(ext_loader, op_state)));
    if let Some(snapshotted_data) = maybe_snapshotted_data {
      let scope =
        &mut v8::HandleScope::with_context(&mut isolate, global_context);
      let mut module_map = module_map_rc.borrow_mut();
      module_map.update_with_snapshotted_data(scope, snapshotted_data);
    }
    isolate.set_data(
      Self::MODULE_MAP_DATA_OFFSET,
      Rc::into_raw(module_map_rc.clone()) as *mut c_void,
    );

    let mut js_runtime = Self {
      v8_isolate: Some(isolate),
      snapshot_options,
      allocations: IsolateAllocations::default(),
      event_loop_middlewares: Vec::with_capacity(num_extensions),
      extensions,
      state: state_rc,
      module_map: Some(module_map_rc.clone()),
      is_main: options.is_main,
    };

    // Init resources and ops before extensions to make sure they are
    // available during the initialization process.
    js_runtime.init_extension_ops().unwrap();
    let realm = js_runtime.global_realm();
    module_map_rc.borrow().loader.allow_ext_resolution();
    js_runtime.init_extension_js(&realm).unwrap();
    module_map_rc.borrow().loader.disallow_ext_resolution();

    js_runtime
  }

  fn drop_state_and_module_map(v8_isolate: &mut OwnedIsolate) {
    let state_ptr = v8_isolate.get_data(Self::STATE_DATA_OFFSET);
    let state_rc =
    // SAFETY: We are sure that it's a valid pointer for whole lifetime of
    // the runtime.
    unsafe { Rc::from_raw(state_ptr as *const RefCell<JsRuntimeState>) };
    drop(state_rc);

    let module_map_ptr = v8_isolate.get_data(Self::MODULE_MAP_DATA_OFFSET);
    let module_map_rc =
    // SAFETY: We are sure that it's a valid pointer for whole lifetime of
    // the runtime.
    unsafe { Rc::from_raw(module_map_ptr as *const RefCell<ModuleMap>) };
    drop(module_map_rc);
  }

  #[inline]
  fn get_module_map(&mut self) -> &Rc<RefCell<ModuleMap>> {
    self.module_map.as_ref().unwrap()
  }

  #[inline]
  pub fn global_context(&mut self) -> v8::Global<v8::Context> {
    let state = self.state.borrow();
    let global_realm = state.global_realm.as_ref().unwrap();
    global_realm.context().clone()
  }

  #[inline]
  pub fn v8_isolate(&mut self) -> &mut v8::OwnedIsolate {
    self.v8_isolate.as_mut().unwrap()
  }

  #[inline]
  pub fn inspector(&mut self) -> Rc<RefCell<JsRuntimeInspector>> {
    self.state.borrow().inspector()
  }

  #[inline]
  pub fn global_realm(&mut self) -> JsRealm {
    let state = self.state.borrow();
    state.global_realm.clone().unwrap()
  }

  /// Creates a new realm (V8 context) in this JS execution context,
  /// pre-initialized with all of the extensions that were passed in
  /// [`RuntimeOptions::extensions`] when the [`JsRuntime`] was
  /// constructed.
  pub fn create_realm(&mut self) -> Result<JsRealm, Error> {
    let realm = {
      let realm_idx = self.state.borrow().known_realms.len() as u16;

      let op_ctxs: Box<[OpCtx]> = self
        .global_realm()
        .state(self.v8_isolate())
        .borrow()
        .op_ctxs
        .iter()
        .map(|op_ctx| {
          OpCtx::new(
            op_ctx.id,
            realm_idx,
            op_ctx.decl.clone(),
            op_ctx.state.clone(),
            op_ctx.runtime_state.clone(),
          )
        })
        .collect();

      // SAFETY: Having the scope tied to self's lifetime makes it impossible to
      // reference JsRuntimeState::op_ctxs while the scope is alive. Here we
      // turn it into an unbound lifetime, which is sound because 1. it only
      // lives until the end of this block, and 2. the HandleScope only has
      // access to the isolate, and nothing else we're accessing from self does.
      let scope = &mut v8::HandleScope::new(unsafe {
        &mut *(self.v8_isolate() as *mut v8::OwnedIsolate)
      });
      let context =
        bindings::initialize_context(scope, &op_ctxs, self.snapshot_options);
      context.set_slot(
        scope,
        Rc::new(RefCell::new(ContextState {
          op_ctxs,
          ..Default::default()
        })),
      );

      self
        .state
        .borrow_mut()
        .known_realms
        .push(v8::Weak::new(scope, context));

      JsRealm::new(v8::Global::new(scope, context))
    };

    self
      .module_map
      .as_ref()
      .unwrap()
      .borrow()
      .loader
      .allow_ext_resolution();
    self.init_extension_js(&realm)?;
    self
      .module_map
      .as_ref()
      .unwrap()
      .borrow()
      .loader
      .disallow_ext_resolution();
    Ok(realm)
  }

  #[inline]
  pub fn handle_scope(&mut self) -> v8::HandleScope {
    self.global_realm().handle_scope(self.v8_isolate())
  }

  fn setup_isolate(mut isolate: v8::OwnedIsolate) -> v8::OwnedIsolate {
    isolate.set_capture_stack_trace_for_uncaught_exceptions(true, 10);
    isolate.set_promise_reject_callback(bindings::promise_reject_callback);
    isolate.set_host_initialize_import_meta_object_callback(
      bindings::host_initialize_import_meta_object_callback,
    );
    isolate.set_host_import_module_dynamically_callback(
      bindings::host_import_module_dynamically_callback,
    );
    isolate.set_wasm_async_resolve_promise_callback(
      bindings::wasm_async_resolve_promise_callback,
    );
    isolate
  }

  pub(crate) fn state(isolate: &v8::Isolate) -> Rc<RefCell<JsRuntimeState>> {
    let state_ptr = isolate.get_data(Self::STATE_DATA_OFFSET);
    let state_rc =
      // SAFETY: We are sure that it's a valid pointer for whole lifetime of
      // the runtime.
      unsafe { Rc::from_raw(state_ptr as *const RefCell<JsRuntimeState>) };
    let state = state_rc.clone();
    let _ = Rc::into_raw(state_rc);
    state
  }

  pub(crate) fn module_map(isolate: &v8::Isolate) -> Rc<RefCell<ModuleMap>> {
    let module_map_ptr = isolate.get_data(Self::MODULE_MAP_DATA_OFFSET);
    let module_map_rc =
      // SAFETY: We are sure that it's a valid pointer for whole lifetime of
      // the runtime.
      unsafe { Rc::from_raw(module_map_ptr as *const RefCell<ModuleMap>) };
    let module_map = module_map_rc.clone();
    let _ = Rc::into_raw(module_map_rc);
    module_map
  }

  /// Initializes JS of provided Extensions in the given realm
  fn init_extension_js(&mut self, realm: &JsRealm) -> Result<(), Error> {
    fn load_and_evaluate_module(
      runtime: &mut JsRuntime,
      file_source: &ExtensionFileSource,
    ) -> Result<(), Error> {
      futures::executor::block_on(async {
        let id = runtime
          .load_side_module(
            &ModuleSpecifier::parse(file_source.specifier)?,
            None,
          )
          .await?;
        let receiver = runtime.mod_evaluate(id);
        runtime.run_event_loop(false).await?;
        receiver.await?
      })
      .with_context(|| format!("Couldn't execute '{}'", file_source.specifier))
    }

    // Take extensions to avoid double-borrow
    let extensions = std::mem::take(&mut self.extensions);
    for ext in extensions.borrow().iter() {
      {
        if let Some(esm_files) = ext.get_esm_sources() {
          if let Some(entry_point) = ext.get_esm_entry_point() {
            let file_source = esm_files
              .iter()
              .find(|file| file.specifier == entry_point)
              .unwrap();
            load_and_evaluate_module(self, file_source)?;
          } else {
            for file_source in esm_files {
              load_and_evaluate_module(self, file_source)?;
            }
          }
        }
      }

      {
        if let Some(js_files) = ext.get_js_sources() {
          for file_source in js_files {
            realm.execute_script(
              self.v8_isolate(),
              file_source.specifier,
              file_source.load()?,
            )?;
          }
        }
      }

      // TODO(bartlomieju): this not great that we need to have this conditional
      // here, but I haven't found a better way to do it yet.
      if ext.is_core {
        self.init_cbs(realm);
      }
    }
    // Restore extensions
    self.extensions = extensions;

    Ok(())
  }

  /// Collects ops from extensions & applies middleware
  fn collect_ops(exts: &mut [Extension]) -> Vec<OpDecl> {
    for (ext, previous_exts) in
      exts.iter().enumerate().map(|(i, ext)| (ext, &exts[..i]))
    {
      ext.check_dependencies(previous_exts);
    }

    // Middleware
    let middleware: Vec<Box<OpMiddlewareFn>> = exts
      .iter_mut()
      .filter_map(|e| e.init_middleware())
      .collect();

    // macroware wraps an opfn in all the middleware
    let macroware = move |d| middleware.iter().fold(d, |d, m| m(d));

    // Flatten ops, apply middlware & override disabled ops
    exts
      .iter_mut()
      .filter_map(|e| e.init_ops())
      .flatten()
      .map(|d| OpDecl {
        name: d.name,
        ..macroware(d)
      })
      .collect()
  }

  /// Initializes ops of provided Extensions
  fn init_extension_ops(&mut self) -> Result<(), Error> {
    let op_state = self.op_state();
    // Setup state
    for e in self.extensions.borrow_mut().iter_mut() {
      // ops are already registered during in bindings::initialize_context();
      e.init_state(&mut op_state.borrow_mut());

      // Setup event-loop middleware
      if let Some(middleware) = e.init_event_loop_middleware() {
        self.event_loop_middlewares.push(middleware);
      }
    }
    Ok(())
  }

  pub fn eval<'s, T>(
    scope: &mut v8::HandleScope<'s>,
    code: &str,
  ) -> Option<v8::Local<'s, T>>
  where
    v8::Local<'s, T>: TryFrom<v8::Local<'s, v8::Value>, Error = v8::DataError>,
  {
    let scope = &mut v8::EscapableHandleScope::new(scope);
    let source = v8::String::new(scope, code).unwrap();
    let script = v8::Script::compile(scope, source, None).unwrap();
    let v = script.run(scope)?;
    scope.escape(v).try_into().ok()
  }

  /// Grabs a reference to core.js' eventLoopTick & buildCustomError
  fn init_cbs(&mut self, realm: &JsRealm) {
    let (event_loop_tick_cb, build_custom_error_cb) = {
      let scope = &mut realm.handle_scope(self.v8_isolate());
      let context = realm.context();
      let context_local = v8::Local::new(scope, context);
      let global = context_local.global(scope);
      let deno_str =
        v8::String::new_external_onebyte_static(scope, b"Deno").unwrap();
      let core_str =
        v8::String::new_external_onebyte_static(scope, b"core").unwrap();
      let event_loop_tick_str =
        v8::String::new_external_onebyte_static(scope, b"eventLoopTick")
          .unwrap();
      let build_custom_error_str =
        v8::String::new_external_onebyte_static(scope, b"buildCustomError")
          .unwrap();

      let deno_obj: v8::Local<v8::Object> = global
        .get(scope, deno_str.into())
        .unwrap()
        .try_into()
        .unwrap();
      let core_obj: v8::Local<v8::Object> = deno_obj
        .get(scope, core_str.into())
        .unwrap()
        .try_into()
        .unwrap();

      let event_loop_tick_cb: v8::Local<v8::Function> = core_obj
        .get(scope, event_loop_tick_str.into())
        .unwrap()
        .try_into()
        .unwrap();
      let build_custom_error_cb: v8::Local<v8::Function> = core_obj
        .get(scope, build_custom_error_str.into())
        .unwrap()
        .try_into()
        .unwrap();
      (
        v8::Global::new(scope, event_loop_tick_cb),
        v8::Global::new(scope, build_custom_error_cb),
      )
    };

    // Put global handles in the realm's ContextState
    let state_rc = realm.state(self.v8_isolate());
    let mut state = state_rc.borrow_mut();
    state
      .js_event_loop_tick_cb
      .replace(Rc::new(event_loop_tick_cb));
    state
      .js_build_custom_error_cb
      .replace(Rc::new(build_custom_error_cb));
  }

  /// Returns the runtime's op state, which can be used to maintain ops
  /// and access resources between op calls.
  pub fn op_state(&mut self) -> Rc<RefCell<OpState>> {
    let state = self.state.borrow();
    state.op_state.clone()
  }

  /// Executes traditional JavaScript code (traditional = not ES modules).
  ///
  /// The execution takes place on the current global context, so it is possible
  /// to maintain local JS state and invoke this method multiple times.
  ///
  /// `name` can be a filepath or any other string, but it is required to be 7-bit ASCII, eg.
  ///
  ///   - "/some/file/path.js"
  ///   - "<anon>"
  ///   - "[native code]"
  ///
  /// The same `name` value can be used for multiple executions.
  ///
  /// `Error` can usually be downcast to `JsError`.
  pub fn execute_script(
    &mut self,
    name: &'static str,
    source_code: ModuleCode,
  ) -> Result<v8::Global<v8::Value>, Error> {
    self
      .global_realm()
      .execute_script(self.v8_isolate(), name, source_code)
  }

  /// Executes traditional JavaScript code (traditional = not ES modules).
  ///
  /// The execution takes place on the current global context, so it is possible
  /// to maintain local JS state and invoke this method multiple times.
  ///
  /// `name` can be a filepath or any other string, but it is required to be 7-bit ASCII, eg.
  ///
  ///   - "/some/file/path.js"
  ///   - "<anon>"
  ///   - "[native code]"
  ///
  /// The same `name` value can be used for multiple executions.
  ///
  /// `Error` can usually be downcast to `JsError`.
  pub fn execute_script_static(
    &mut self,
    name: &'static str,
    source_code: &'static str,
  ) -> Result<v8::Global<v8::Value>, Error> {
    self.global_realm().execute_script(
      self.v8_isolate(),
      name,
      ModuleCode::from_static(source_code),
    )
  }

  /// Call a function. If it returns a promise, run the event loop until that
  /// promise is settled. If the promise rejects or there is an uncaught error
  /// in the event loop, return `Err(error)`. Or return `Ok(<await returned>)`.
  pub async fn call_and_await(
    &mut self,
    function: &v8::Global<v8::Function>,
  ) -> Result<v8::Global<v8::Value>, Error> {
    let promise = {
      let scope = &mut self.handle_scope();
      let cb = function.open(scope);
      let this = v8::undefined(scope).into();
      let promise = cb.call(scope, this, &[]);
      if promise.is_none() || scope.is_execution_terminating() {
        let undefined = v8::undefined(scope).into();
        return exception_to_err_result(scope, undefined, false);
      }
      v8::Global::new(scope, promise.unwrap())
    };
    self.resolve_value(promise).await
  }

  /// Takes a snapshot. The isolate should have been created with will_snapshot
  /// set to true.
  ///
  /// `Error` can usually be downcast to `JsError`.
  pub fn snapshot(mut self) -> v8::StartupData {
    self.state.borrow_mut().inspector.take();

    // Set the context to be snapshot's default context
    {
      let context = self.global_context();
      let mut scope = self.handle_scope();
      let local_context = v8::Local::new(&mut scope, context);
      scope.set_default_context(local_context);
    }

    // Serialize the module map and store its data in the snapshot.
    {
      let snapshotted_data = {
        let module_map_rc = self.module_map.take().unwrap();
        let module_map = module_map_rc.borrow();
        module_map.serialize_for_snapshotting(&mut self.handle_scope())
      };

      let context = self.global_context();
      let mut scope = self.handle_scope();
      snapshot_util::set_snapshotted_data(
        &mut scope,
        context,
        snapshotted_data,
      );
    }

    // Drop existing ModuleMap to drop v8::Global handles
    {
      let v8_isolate = self.v8_isolate();
      Self::drop_state_and_module_map(v8_isolate);
    }

    // Drop other v8::Global handles before snapshotting
    {
      for weak_context in &self.state.clone().borrow().known_realms {
        let scope = &mut self.handle_scope();
        if let Some(context) = weak_context.to_local(scope) {
          let realm = JsRealmLocal::new(context);
          let realm_state_rc = realm.state(scope);
          let mut realm_state = realm_state_rc.borrow_mut();
          std::mem::take(&mut realm_state.js_event_loop_tick_cb);
          std::mem::take(&mut realm_state.js_build_custom_error_cb);
          std::mem::take(&mut realm_state.js_promise_reject_cb);
          std::mem::take(&mut realm_state.js_format_exception_cb);
          std::mem::take(&mut realm_state.js_wasm_streaming_cb);
          context.clear_all_slots(scope);
        }
      }

      let mut state = self.state.borrow_mut();
      state.known_realms.clear();
      state.global_realm.take();
    }

    let snapshot_creator = self.v8_isolate.take().unwrap();
    snapshot_creator
      .create_blob(v8::FunctionCodeHandling::Keep)
      .unwrap()
  }

  /// Returns the namespace object of a module.
  ///
  /// This is only available after module evaluation has completed.
  /// This function panics if module has not been instantiated.
  pub fn get_module_namespace(
    &mut self,
    module_id: ModuleId,
  ) -> Result<v8::Global<v8::Object>, Error> {
    let module_map_rc = Self::module_map(self.v8_isolate());

    let module_handle = module_map_rc
      .borrow()
      .get_handle(module_id)
      .expect("ModuleInfo not found");

    let scope = &mut self.handle_scope();

    let module = module_handle.open(scope);

    if module.get_status() == v8::ModuleStatus::Errored {
      let exception = module.get_exception();
      return exception_to_err_result(scope, exception, false);
    }

    assert!(matches!(
      module.get_status(),
      v8::ModuleStatus::Instantiated | v8::ModuleStatus::Evaluated
    ));

    let module_namespace: v8::Local<v8::Object> =
      v8::Local::try_from(module.get_module_namespace())
        .map_err(|err: v8::DataError| generic_error(err.to_string()))?;

    Ok(v8::Global::new(scope, module_namespace))
  }

  /// Registers a callback on the isolate when the memory limits are approached.
  /// Use this to prevent V8 from crashing the process when reaching the limit.
  ///
  /// Calls the closure with the current heap limit and the initial heap limit.
  /// The return value of the closure is set as the new limit.
  pub fn add_near_heap_limit_callback<C>(&mut self, cb: C)
  where
    C: FnMut(usize, usize) -> usize + 'static,
  {
    let boxed_cb = Box::new(RefCell::new(cb));
    let data = boxed_cb.as_ptr() as *mut c_void;

    let prev = self
      .allocations
      .near_heap_limit_callback_data
      .replace((boxed_cb, near_heap_limit_callback::<C>));
    if let Some((_, prev_cb)) = prev {
      self
        .v8_isolate()
        .remove_near_heap_limit_callback(prev_cb, 0);
    }

    self
      .v8_isolate()
      .add_near_heap_limit_callback(near_heap_limit_callback::<C>, data);
  }

  pub fn remove_near_heap_limit_callback(&mut self, heap_limit: usize) {
    if let Some((_, cb)) = self.allocations.near_heap_limit_callback_data.take()
    {
      self
        .v8_isolate()
        .remove_near_heap_limit_callback(cb, heap_limit);
    }
  }

  fn pump_v8_message_loop(&mut self) -> Result<(), Error> {
    let scope = &mut self.handle_scope();
    while v8::Platform::pump_message_loop(
      &v8::V8::get_current_platform(),
      scope,
      false, // don't block if there are no tasks
    ) {
      // do nothing
    }

    let tc_scope = &mut v8::TryCatch::new(scope);
    tc_scope.perform_microtask_checkpoint();
    match tc_scope.exception() {
      None => Ok(()),
      Some(exception) => exception_to_err_result(tc_scope, exception, false),
    }
  }

  pub fn maybe_init_inspector(&mut self) {
    if self.state.borrow().inspector.is_some() {
      return;
    }

    let global_context = self.global_context();
    let mut state = self.state.borrow_mut();
    state.inspector = Some(JsRuntimeInspector::new(
      self.v8_isolate.as_mut().unwrap(),
      global_context,
      self.is_main,
    ));
  }

  pub fn poll_value(
    &mut self,
    global: &v8::Global<v8::Value>,
    cx: &mut Context,
  ) -> Poll<Result<v8::Global<v8::Value>, Error>> {
    let state = self.poll_event_loop(cx, false);

    let mut scope = self.handle_scope();
    let local = v8::Local::<v8::Value>::new(&mut scope, global);

    if let Ok(promise) = v8::Local::<v8::Promise>::try_from(local) {
      match promise.state() {
        v8::PromiseState::Pending => match state {
          Poll::Ready(Ok(_)) => {
            let msg = "Promise resolution is still pending but the event loop has already resolved.";
            Poll::Ready(Err(generic_error(msg)))
          }
          Poll::Ready(Err(e)) => Poll::Ready(Err(e)),
          Poll::Pending => Poll::Pending,
        },
        v8::PromiseState::Fulfilled => {
          let value = promise.result(&mut scope);
          let value_handle = v8::Global::new(&mut scope, value);
          Poll::Ready(Ok(value_handle))
        }
        v8::PromiseState::Rejected => {
          let exception = promise.result(&mut scope);
          Poll::Ready(exception_to_err_result(&mut scope, exception, false))
        }
      }
    } else {
      let value_handle = v8::Global::new(&mut scope, local);
      Poll::Ready(Ok(value_handle))
    }
  }

  /// Waits for the given value to resolve while polling the event loop.
  ///
  /// This future resolves when either the value is resolved or the event loop runs to
  /// completion.
  pub async fn resolve_value(
    &mut self,
    global: v8::Global<v8::Value>,
  ) -> Result<v8::Global<v8::Value>, Error> {
    poll_fn(|cx| self.poll_value(&global, cx)).await
  }

  /// Runs event loop to completion
  ///
  /// This future resolves when:
  ///  - there are no more pending dynamic imports
  ///  - there are no more pending ops
  ///  - there are no more active inspector sessions (only if `wait_for_inspector` is set to true)
  pub async fn run_event_loop(
    &mut self,
    wait_for_inspector: bool,
  ) -> Result<(), Error> {
    poll_fn(|cx| self.poll_event_loop(cx, wait_for_inspector)).await
  }

  /// Runs a single tick of event loop
  ///
  /// If `wait_for_inspector` is set to true event loop
  /// will return `Poll::Pending` if there are active inspector sessions.
  pub fn poll_event_loop(
    &mut self,
    cx: &mut Context,
    wait_for_inspector: bool,
  ) -> Poll<Result<(), Error>> {
    let has_inspector: bool;

    {
      let state = self.state.borrow();
      has_inspector = state.inspector.is_some();
      state.waker.register(cx.waker());
    }

    if has_inspector {
      // We poll the inspector first.
      let _ = self.inspector().borrow().poll_sessions(Some(cx)).unwrap();
    }

    self.pump_v8_message_loop()?;

    // Dynamic module loading - ie. modules loaded using "import()"
    {
      // Run in a loop so that dynamic imports that only depend on another
      // dynamic import can be resolved in this event loop iteration.
      //
      // For example, a dynamically imported module like the following can be
      // immediately resolved after `dependency.ts` is fully evaluated, but it
      // wouldn't if not for this loop.
      //
      //    await delay(1000);
      //    await import("./dependency.ts");
      //    console.log("test")
      //
      loop {
        let poll_imports = self.prepare_dyn_imports(cx)?;
        assert!(poll_imports.is_ready());

        let poll_imports = self.poll_dyn_imports(cx)?;
        assert!(poll_imports.is_ready());

        if !self.evaluate_dyn_imports() {
          break;
        }
      }
    }

    // Resolve async ops, run all next tick callbacks and macrotasks callbacks
    // and only then check for any promise exceptions (`unhandledrejection`
    // handlers are run in macrotasks callbacks so we need to let them run
    // first).
    self.do_js_event_loop_tick(cx)?;
    self.check_promise_rejections()?;

    // Event loop middlewares
    let mut maybe_scheduling = false;
    {
      let op_state = self.state.borrow().op_state.clone();
      for f in &self.event_loop_middlewares {
        if f(op_state.clone(), cx) {
          maybe_scheduling = true;
        }
      }
    }

    // Top level module
    self.evaluate_pending_module();

    let pending_state = self.event_loop_pending_state();
    if !pending_state.is_pending() && !maybe_scheduling {
      if has_inspector {
        let inspector = self.inspector();
        let has_active_sessions = inspector.borrow().has_active_sessions();
        let has_blocking_sessions = inspector.borrow().has_blocking_sessions();

        if wait_for_inspector && has_active_sessions {
          // If there are no blocking sessions (eg. REPL) we can now notify
          // debugger that the program has finished running and we're ready
          // to exit the process once debugger disconnects.
          if !has_blocking_sessions {
            let context = self.global_context();
            let scope = &mut self.handle_scope();
            inspector.borrow_mut().context_destroyed(scope, context);
            println!("Program finished. Waiting for inspector to disconnect to exit the process...");
          }

          return Poll::Pending;
        }
      }

      return Poll::Ready(Ok(()));
    }

    let state = self.state.borrow();

    // Check if more async ops have been dispatched
    // during this turn of event loop.
    // If there are any pending background tasks, we also wake the runtime to
    // make sure we don't miss them.
    // TODO(andreubotella) The event loop will spin as long as there are pending
    // background tasks. We should look into having V8 notify us when a
    // background task is done.
    if state.have_unpolled_ops
      || pending_state.has_pending_background_tasks
      || pending_state.has_tick_scheduled
      || maybe_scheduling
    {
      state.waker.wake();
    }

    drop(state);

    if pending_state.has_pending_module_evaluation {
      if pending_state.has_pending_refed_ops
        || pending_state.has_pending_dyn_imports
        || pending_state.has_pending_dyn_module_evaluation
        || pending_state.has_pending_background_tasks
        || pending_state.has_tick_scheduled
        || maybe_scheduling
      {
        // pass, will be polled again
      } else {
        let scope = &mut self.handle_scope();
        let messages = find_stalled_top_level_await(scope);
        // We are gonna print only a single message to provide a nice formatting
        // with source line of offending promise shown. Once user fixed it, then
        // they will get another error message for the next promise (but this
        // situation is gonna be very rare, if ever happening).
        assert!(!messages.is_empty());
        let msg = v8::Local::new(scope, messages[0].clone());
        let js_error = JsError::from_v8_message(scope, msg);
        return Poll::Ready(Err(js_error.into()));
      }
    }

    if pending_state.has_pending_dyn_module_evaluation {
      if pending_state.has_pending_refed_ops
        || pending_state.has_pending_dyn_imports
        || pending_state.has_pending_background_tasks
        || pending_state.has_tick_scheduled
      {
        // pass, will be polled again
      } else if self.state.borrow().dyn_module_evaluate_idle_counter >= 1 {
        let scope = &mut self.handle_scope();
        let messages = find_stalled_top_level_await(scope);
        // We are gonna print only a single message to provide a nice formatting
        // with source line of offending promise shown. Once user fixed it, then
        // they will get another error message for the next promise (but this
        // situation is gonna be very rare, if ever happening).
        assert!(!messages.is_empty());
        let msg = v8::Local::new(scope, messages[0].clone());
        let js_error = JsError::from_v8_message(scope, msg);
        return Poll::Ready(Err(js_error.into()));
      } else {
        let mut state = self.state.borrow_mut();
        // Delay the above error by one spin of the event loop. A dynamic import
        // evaluation may complete during this, in which case the counter will
        // reset.
        state.dyn_module_evaluate_idle_counter += 1;
        state.waker.wake();
      }
    }

    Poll::Pending
  }

  fn event_loop_pending_state(&mut self) -> EventLoopPendingState {
    let isolate = self.v8_isolate.as_mut().unwrap();
    let mut scope = v8::HandleScope::new(isolate);
    EventLoopPendingState::new(
      &mut scope,
      &mut self.state.borrow_mut(),
      &self.module_map.as_ref().unwrap().borrow(),
    )
  }

  pub(crate) fn event_loop_pending_state_from_scope(
    scope: &mut v8::HandleScope,
  ) -> EventLoopPendingState {
    let state = Self::state(scope);
    let module_map = Self::module_map(scope);
    let state = EventLoopPendingState::new(
      scope,
      &mut state.borrow_mut(),
      &module_map.borrow(),
    );
    state
  }
}

fn get_stalled_top_level_await_message_for_module(
  scope: &mut v8::HandleScope,
  module_id: ModuleId,
) -> Vec<v8::Global<v8::Message>> {
  let module_map = JsRuntime::module_map(scope);
  let module_map = module_map.borrow();
  let module_handle = module_map.handles.get(module_id).unwrap();

  let module = v8::Local::new(scope, module_handle);
  let stalled = module.get_stalled_top_level_await_message(scope);
  let mut messages = vec![];
  for (_, message) in stalled {
    messages.push(v8::Global::new(scope, message));
  }
  messages
}

fn find_stalled_top_level_await(
  scope: &mut v8::HandleScope,
) -> Vec<v8::Global<v8::Message>> {
  let module_map = JsRuntime::module_map(scope);
  let module_map = module_map.borrow();

  // First check if that's root module
  let root_module_id = module_map
    .info
    .iter()
    .filter(|m| m.main)
    .map(|m| m.id)
    .next();

  if let Some(root_module_id) = root_module_id {
    let messages =
      get_stalled_top_level_await_message_for_module(scope, root_module_id);
    if !messages.is_empty() {
      return messages;
    }
  }

  // It wasn't a top module, so iterate over all modules and try to find
  // any with stalled top level await
  for module_id in 0..module_map.handles.len() {
    let messages =
      get_stalled_top_level_await_message_for_module(scope, module_id);
    if !messages.is_empty() {
      return messages;
    }
  }

  unreachable!()
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct EventLoopPendingState {
  has_pending_refed_ops: bool,
  has_pending_dyn_imports: bool,
  has_pending_dyn_module_evaluation: bool,
  has_pending_module_evaluation: bool,
  has_pending_background_tasks: bool,
  has_tick_scheduled: bool,
}
impl EventLoopPendingState {
  pub fn new(
    scope: &mut v8::HandleScope<()>,
    state: &mut JsRuntimeState,
    module_map: &ModuleMap,
  ) -> EventLoopPendingState {
    let mut num_unrefed_ops = 0;

    if state.known_realms.len() == 1 {
      let realm = state.global_realm.as_ref().unwrap();
      num_unrefed_ops += realm.state(scope).borrow().unrefed_ops.len();
    } else {
      for weak_context in &state.known_realms {
        if let Some(context) = weak_context.to_local(scope) {
          let realm = JsRealmLocal::new(context);
          num_unrefed_ops += realm.state(scope).borrow().unrefed_ops.len();
        }
      }
    }

    EventLoopPendingState {
      has_pending_refed_ops: state.pending_ops.len() > num_unrefed_ops,
      has_pending_dyn_imports: module_map.has_pending_dynamic_imports(),
      has_pending_dyn_module_evaluation: !state
        .pending_dyn_mod_evaluate
        .is_empty(),
      has_pending_module_evaluation: state.pending_mod_evaluate.is_some(),
      has_pending_background_tasks: scope.has_pending_background_tasks(),
      has_tick_scheduled: state.has_tick_scheduled,
    }
  }

  pub fn is_pending(&self) -> bool {
    self.has_pending_refed_ops
      || self.has_pending_dyn_imports
      || self.has_pending_dyn_module_evaluation
      || self.has_pending_module_evaluation
      || self.has_pending_background_tasks
      || self.has_tick_scheduled
  }
}

extern "C" fn near_heap_limit_callback<F>(
  data: *mut c_void,
  current_heap_limit: usize,
  initial_heap_limit: usize,
) -> usize
where
  F: FnMut(usize, usize) -> usize,
{
  // SAFETY: The data is a pointer to the Rust callback function. It is stored
  // in `JsRuntime::allocations` and thus is guaranteed to outlive the isolate.
  let callback = unsafe { &mut *(data as *mut F) };
  callback(current_heap_limit, initial_heap_limit)
}

impl JsRuntimeState {
  pub(crate) fn inspector(&self) -> Rc<RefCell<JsRuntimeInspector>> {
    self.inspector.as_ref().unwrap().clone()
  }

  /// Called by `bindings::host_import_module_dynamically_callback`
  /// after initiating new dynamic import load.
  pub fn notify_new_dynamic_import(&mut self) {
    // Notify event loop to poll again soon.
    self.waker.wake();
  }
}

pub(crate) fn exception_to_err_result<T>(
  scope: &mut v8::HandleScope,
  exception: v8::Local<v8::Value>,
  in_promise: bool,
) -> Result<T, Error> {
  let state_rc = JsRuntime::state(scope);

  let was_terminating_execution = scope.is_execution_terminating();
  // Disable running microtasks for a moment. When upgrading to V8 v11.4
  // we discovered that canceling termination here will cause the queued
  // microtasks to run which breaks some tests.
  scope.set_microtasks_policy(v8::MicrotasksPolicy::Explicit);
  // If TerminateExecution was called, cancel isolate termination so that the
  // exception can be created. Note that `scope.is_execution_terminating()` may
  // have returned false if TerminateExecution was indeed called but there was
  // no JS to execute after the call.
  scope.cancel_terminate_execution();
  let mut exception = exception;
  {
    // If termination is the result of a `op_dispatch_exception` call, we want
    // to use the exception that was passed to it rather than the exception that
    // was passed to this function.
    let state = state_rc.borrow();
    exception = if let Some(exception) = &state.dispatched_exception {
      v8::Local::new(scope, exception.clone())
    } else if was_terminating_execution && exception.is_null_or_undefined() {
      let message = v8::String::new(scope, "execution terminated").unwrap();
      v8::Exception::error(scope, message)
    } else {
      exception
    };
  }

  let mut js_error = JsError::from_v8_exception(scope, exception);
  if in_promise {
    js_error.exception_message = format!(
      "Uncaught (in promise) {}",
      js_error.exception_message.trim_start_matches("Uncaught ")
    );
  }

  if was_terminating_execution {
    // Resume exception termination.
    scope.terminate_execution();
  }
  scope.set_microtasks_policy(v8::MicrotasksPolicy::Auto);

  Err(js_error.into())
}

// Related to module loading
impl JsRuntime {
  pub(crate) fn instantiate_module(
    &mut self,
    id: ModuleId,
  ) -> Result<(), v8::Global<v8::Value>> {
    let module_map_rc = Self::module_map(self.v8_isolate());
    let scope = &mut self.handle_scope();
    let tc_scope = &mut v8::TryCatch::new(scope);

    let module = module_map_rc
      .borrow()
      .get_handle(id)
      .map(|handle| v8::Local::new(tc_scope, handle))
      .expect("ModuleInfo not found");

    if module.get_status() == v8::ModuleStatus::Errored {
      return Err(v8::Global::new(tc_scope, module.get_exception()));
    }

    // IMPORTANT: No borrows to `ModuleMap` can be held at this point because
    // `module_resolve_callback` will be calling into `ModuleMap` from within
    // the isolate.
    let instantiate_result =
      module.instantiate_module(tc_scope, bindings::module_resolve_callback);

    if instantiate_result.is_none() {
      let exception = tc_scope.exception().unwrap();
      return Err(v8::Global::new(tc_scope, exception));
    }

    Ok(())
  }

  fn dynamic_import_module_evaluate(
    &mut self,
    load_id: ModuleLoadId,
    id: ModuleId,
  ) -> Result<(), Error> {
    let module_map_rc = Self::module_map(self.v8_isolate());

    let module_handle = module_map_rc
      .borrow()
      .get_handle(id)
      .expect("ModuleInfo not found");

    let status = {
      let scope = &mut self.handle_scope();
      let module = module_handle.open(scope);
      module.get_status()
    };

    match status {
      v8::ModuleStatus::Instantiated | v8::ModuleStatus::Evaluated => {}
      _ => return Ok(()),
    }

    // IMPORTANT: Top-level-await is enabled, which means that return value
    // of module evaluation is a promise.
    //
    // This promise is internal, and not the same one that gets returned to
    // the user. We add an empty `.catch()` handler so that it does not result
    // in an exception if it rejects. That will instead happen for the other
    // promise if not handled by the user.
    //
    // For more details see:
    // https://github.com/denoland/deno/issues/4908
    // https://v8.dev/features/top-level-await#module-execution-order
    let global_realm = self.state.borrow_mut().global_realm.clone().unwrap();
    let scope =
      &mut global_realm.handle_scope(self.v8_isolate.as_mut().unwrap());
    let tc_scope = &mut v8::TryCatch::new(scope);
    let module = v8::Local::new(tc_scope, &module_handle);
    let maybe_value = module.evaluate(tc_scope);

    // Update status after evaluating.
    let status = module.get_status();

    if let Some(value) = maybe_value {
      assert!(
        status == v8::ModuleStatus::Evaluated
          || status == v8::ModuleStatus::Errored
      );
      let promise = v8::Local::<v8::Promise>::try_from(value)
        .expect("Expected to get promise as module evaluation result");
      let empty_fn = bindings::create_empty_fn(tc_scope).unwrap();
      promise.catch(tc_scope, empty_fn);
      let promise_global = v8::Global::new(tc_scope, promise);
      let module_global = v8::Global::new(tc_scope, module);

      let dyn_import_mod_evaluate = DynImportModEvaluate {
        load_id,
        module_id: id,
        promise: promise_global,
        module: module_global,
      };

      self
        .state
        .borrow_mut()
        .pending_dyn_mod_evaluate
        .push(dyn_import_mod_evaluate);
    } else if tc_scope.has_terminated() || tc_scope.is_execution_terminating() {
      return Err(
        generic_error("Cannot evaluate dynamically imported module, because JavaScript execution has been terminated.")
      );
    } else {
      assert!(status == v8::ModuleStatus::Errored);
    }

    Ok(())
  }

  // TODO(bartlomieju): make it return `ModuleEvaluationFuture`?
  /// Evaluates an already instantiated ES module.
  ///
  /// Returns a receiver handle that resolves when module promise resolves.
  /// Implementors must manually call [`JsRuntime::run_event_loop`] to drive
  /// module evaluation future.
  ///
  /// `Error` can usually be downcast to `JsError` and should be awaited and
  /// checked after [`JsRuntime::run_event_loop`] completion.
  ///
  /// This function panics if module has not been instantiated.
  pub fn mod_evaluate(
    &mut self,
    id: ModuleId,
  ) -> oneshot::Receiver<Result<(), Error>> {
    let global_realm = self.global_realm();
    let state_rc = self.state.clone();
    let module_map_rc = Self::module_map(self.v8_isolate());
    let scope = &mut self.handle_scope();
    let tc_scope = &mut v8::TryCatch::new(scope);

    let module = module_map_rc
      .borrow()
      .get_handle(id)
      .map(|handle| v8::Local::new(tc_scope, handle))
      .expect("ModuleInfo not found");
    let mut status = module.get_status();
    assert_eq!(
      status,
      v8::ModuleStatus::Instantiated,
      "Module not instantiated {id}"
    );

    let (sender, receiver) = oneshot::channel();

    // IMPORTANT: Top-level-await is enabled, which means that return value
    // of module evaluation is a promise.
    //
    // Because that promise is created internally by V8, when error occurs during
    // module evaluation the promise is rejected, and since the promise has no rejection
    // handler it will result in call to `bindings::promise_reject_callback` adding
    // the promise to pending promise rejection table - meaning JsRuntime will return
    // error on next poll().
    //
    // This situation is not desirable as we want to manually return error at the
    // end of this function to handle it further. It means we need to manually
    // remove this promise from pending promise rejection table.
    //
    // For more details see:
    // https://github.com/denoland/deno/issues/4908
    // https://v8.dev/features/top-level-await#module-execution-order
    {
      let mut state = state_rc.borrow_mut();
      assert!(
        state.pending_mod_evaluate.is_none(),
        "There is already pending top level module evaluation"
      );
      state.pending_mod_evaluate = Some(ModEvaluate {
        promise: None,
        has_evaluated: false,
        handled_promise_rejections: vec![],
        sender,
      });
    }

    let maybe_value = module.evaluate(tc_scope);
    {
      let mut state = state_rc.borrow_mut();
      let pending_mod_evaluate = state.pending_mod_evaluate.as_mut().unwrap();
      pending_mod_evaluate.has_evaluated = true;
    }

    // Update status after evaluating.
    status = module.get_status();

    let has_dispatched_exception =
      state_rc.borrow_mut().dispatched_exception.is_some();
    if has_dispatched_exception {
      // This will be overrided in `exception_to_err_result()`.
      let exception = v8::undefined(tc_scope).into();
      let pending_mod_evaluate = {
        let mut state = state_rc.borrow_mut();
        state.pending_mod_evaluate.take().unwrap()
      };
      pending_mod_evaluate
        .sender
        .send(exception_to_err_result(tc_scope, exception, false))
        .expect("Failed to send module evaluation error.");
    } else if let Some(value) = maybe_value {
      assert!(
        status == v8::ModuleStatus::Evaluated
          || status == v8::ModuleStatus::Errored
      );
      let promise = v8::Local::<v8::Promise>::try_from(value)
        .expect("Expected to get promise as module evaluation result");
      let promise_global = v8::Global::new(tc_scope, promise);
      let mut state = state_rc.borrow_mut();
      {
        let pending_mod_evaluate = state.pending_mod_evaluate.as_ref().unwrap();
        let pending_rejection_was_already_handled = pending_mod_evaluate
          .handled_promise_rejections
          .contains(&promise_global);
        if !pending_rejection_was_already_handled {
          global_realm
            .state(tc_scope)
            .borrow_mut()
            .pending_promise_rejections
            .remove(&promise_global);
        }
      }
      let promise_global = v8::Global::new(tc_scope, promise);
      state.pending_mod_evaluate.as_mut().unwrap().promise =
        Some(promise_global);
      tc_scope.perform_microtask_checkpoint();
    } else if tc_scope.has_terminated() || tc_scope.is_execution_terminating() {
      let pending_mod_evaluate = {
        let mut state = state_rc.borrow_mut();
        state.pending_mod_evaluate.take().unwrap()
      };
      pending_mod_evaluate.sender.send(Err(
        generic_error("Cannot evaluate module, because JavaScript execution has been terminated.")
      )).expect("Failed to send module evaluation error.");
    } else {
      assert!(status == v8::ModuleStatus::Errored);
    }

    receiver
  }

  fn dynamic_import_reject(
    &mut self,
    id: ModuleLoadId,
    exception: v8::Global<v8::Value>,
  ) {
    let module_map_rc = Self::module_map(self.v8_isolate());
    let scope = &mut self.handle_scope();

    let resolver_handle = module_map_rc
      .borrow_mut()
      .dynamic_import_map
      .remove(&id)
      .expect("Invalid dynamic import id");
    let resolver = resolver_handle.open(scope);

    // IMPORTANT: No borrows to `ModuleMap` can be held at this point because
    // rejecting the promise might initiate another `import()` which will
    // in turn call `bindings::host_import_module_dynamically_callback` which
    // will reach into `ModuleMap` from within the isolate.
    let exception = v8::Local::new(scope, exception);
    resolver.reject(scope, exception).unwrap();
    scope.perform_microtask_checkpoint();
  }

  fn dynamic_import_resolve(&mut self, id: ModuleLoadId, mod_id: ModuleId) {
    let state_rc = self.state.clone();
    let module_map_rc = Self::module_map(self.v8_isolate());
    let scope = &mut self.handle_scope();

    let resolver_handle = module_map_rc
      .borrow_mut()
      .dynamic_import_map
      .remove(&id)
      .expect("Invalid dynamic import id");
    let resolver = resolver_handle.open(scope);

    let module = {
      module_map_rc
        .borrow()
        .get_handle(mod_id)
        .map(|handle| v8::Local::new(scope, handle))
        .expect("Dyn import module info not found")
    };
    // Resolution success
    assert_eq!(module.get_status(), v8::ModuleStatus::Evaluated);

    // IMPORTANT: No borrows to `ModuleMap` can be held at this point because
    // resolving the promise might initiate another `import()` which will
    // in turn call `bindings::host_import_module_dynamically_callback` which
    // will reach into `ModuleMap` from within the isolate.
    let module_namespace = module.get_module_namespace();
    resolver.resolve(scope, module_namespace).unwrap();
    state_rc.borrow_mut().dyn_module_evaluate_idle_counter = 0;
    scope.perform_microtask_checkpoint();
  }

  fn prepare_dyn_imports(
    &mut self,
    cx: &mut Context,
  ) -> Poll<Result<(), Error>> {
    if self
      .get_module_map()
      .borrow()
      .preparing_dynamic_imports
      .is_empty()
    {
      return Poll::Ready(Ok(()));
    }

    let module_map_rc = self.get_module_map().clone();

    loop {
      let poll_result = module_map_rc
        .borrow_mut()
        .preparing_dynamic_imports
        .poll_next_unpin(cx);

      if let Poll::Ready(Some(prepare_poll)) = poll_result {
        let dyn_import_id = prepare_poll.0;
        let prepare_result = prepare_poll.1;

        match prepare_result {
          Ok(load) => {
            module_map_rc
              .borrow_mut()
              .pending_dynamic_imports
              .push(load.into_future());
          }
          Err(err) => {
            let exception = to_v8_type_error(&mut self.handle_scope(), err);
            self.dynamic_import_reject(dyn_import_id, exception);
          }
        }
        // Continue polling for more prepared dynamic imports.
        continue;
      }

      // There are no active dynamic import loads, or none are ready.
      return Poll::Ready(Ok(()));
    }
  }

  fn poll_dyn_imports(&mut self, cx: &mut Context) -> Poll<Result<(), Error>> {
    if self
      .get_module_map()
      .borrow()
      .pending_dynamic_imports
      .is_empty()
    {
      return Poll::Ready(Ok(()));
    }

    let module_map_rc = self.get_module_map().clone();

    loop {
      let poll_result = module_map_rc
        .borrow_mut()
        .pending_dynamic_imports
        .poll_next_unpin(cx);

      if let Poll::Ready(Some(load_stream_poll)) = poll_result {
        let maybe_result = load_stream_poll.0;
        let mut load = load_stream_poll.1;
        let dyn_import_id = load.id;

        if let Some(load_stream_result) = maybe_result {
          match load_stream_result {
            Ok((request, info)) => {
              // A module (not necessarily the one dynamically imported) has been
              // fetched. Create and register it, and if successful, poll for the
              // next recursive-load event related to this dynamic import.
              let register_result = load.register_and_recurse(
                &mut self.handle_scope(),
                &request,
                info,
              );

              match register_result {
                Ok(()) => {
                  // Keep importing until it's fully drained
                  module_map_rc
                    .borrow_mut()
                    .pending_dynamic_imports
                    .push(load.into_future());
                }
                Err(err) => {
                  let exception = match err {
                    ModuleError::Exception(e) => e,
                    ModuleError::Other(e) => {
                      to_v8_type_error(&mut self.handle_scope(), e)
                    }
                  };
                  self.dynamic_import_reject(dyn_import_id, exception)
                }
              }
            }
            Err(err) => {
              // A non-javascript error occurred; this could be due to a an invalid
              // module specifier, or a problem with the source map, or a failure
              // to fetch the module source code.
              let exception = to_v8_type_error(&mut self.handle_scope(), err);
              self.dynamic_import_reject(dyn_import_id, exception);
            }
          }
        } else {
          // The top-level module from a dynamic import has been instantiated.
          // Load is done.
          let module_id =
            load.root_module_id.expect("Root module should be loaded");
          let result = self.instantiate_module(module_id);
          if let Err(exception) = result {
            self.dynamic_import_reject(dyn_import_id, exception);
          }
          self.dynamic_import_module_evaluate(dyn_import_id, module_id)?;
        }

        // Continue polling for more ready dynamic imports.
        continue;
      }

      // There are no active dynamic import loads, or none are ready.
      return Poll::Ready(Ok(()));
    }
  }

  /// "core" runs V8 with Top Level Await enabled. It means that each
  /// module evaluation returns a promise from V8.
  /// Feature docs: https://v8.dev/features/top-level-await
  ///
  /// This promise resolves after all dependent modules have also
  /// resolved. Each dependent module may perform calls to "import()" and APIs
  /// using async ops will add futures to the runtime's event loop.
  /// It means that the promise returned from module evaluation will
  /// resolve only after all futures in the event loop are done.
  ///
  /// Thus during turn of event loop we need to check if V8 has
  /// resolved or rejected the promise. If the promise is still pending
  /// then another turn of event loop must be performed.
  fn evaluate_pending_module(&mut self) {
    let maybe_module_evaluation =
      self.state.borrow_mut().pending_mod_evaluate.take();

    if maybe_module_evaluation.is_none() {
      return;
    }

    let mut module_evaluation = maybe_module_evaluation.unwrap();
    let state_rc = self.state.clone();
    let scope = &mut self.handle_scope();

    let promise_global = module_evaluation.promise.clone().unwrap();
    let promise = promise_global.open(scope);
    let promise_state = promise.state();

    match promise_state {
      v8::PromiseState::Pending => {
        // NOTE: `poll_event_loop` will decide if
        // runtime would be woken soon
        state_rc.borrow_mut().pending_mod_evaluate = Some(module_evaluation);
      }
      v8::PromiseState::Fulfilled => {
        scope.perform_microtask_checkpoint();
        // Receiver end might have been already dropped, ignore the result
        let _ = module_evaluation.sender.send(Ok(()));
        module_evaluation.handled_promise_rejections.clear();
      }
      v8::PromiseState::Rejected => {
        let exception = promise.result(scope);
        scope.perform_microtask_checkpoint();

        // Receiver end might have been already dropped, ignore the result
        if module_evaluation
          .handled_promise_rejections
          .contains(&promise_global)
        {
          let _ = module_evaluation.sender.send(Ok(()));
          module_evaluation.handled_promise_rejections.clear();
        } else {
          let _ = module_evaluation
            .sender
            .send(exception_to_err_result(scope, exception, false));
        }
      }
    }
  }

  // Returns true if some dynamic import was resolved.
  fn evaluate_dyn_imports(&mut self) -> bool {
    let pending =
      std::mem::take(&mut self.state.borrow_mut().pending_dyn_mod_evaluate);
    if pending.is_empty() {
      return false;
    }
    let mut resolved_any = false;
    let mut still_pending = vec![];
    for pending_dyn_evaluate in pending {
      let maybe_result = {
        let scope = &mut self.handle_scope();

        let module_id = pending_dyn_evaluate.module_id;
        let promise = pending_dyn_evaluate.promise.open(scope);
        let _module = pending_dyn_evaluate.module.open(scope);
        let promise_state = promise.state();

        match promise_state {
          v8::PromiseState::Pending => {
            still_pending.push(pending_dyn_evaluate);
            None
          }
          v8::PromiseState::Fulfilled => {
            Some(Ok((pending_dyn_evaluate.load_id, module_id)))
          }
          v8::PromiseState::Rejected => {
            let exception = promise.result(scope);
            let exception = v8::Global::new(scope, exception);
            Some(Err((pending_dyn_evaluate.load_id, exception)))
          }
        }
      };

      if let Some(result) = maybe_result {
        resolved_any = true;
        match result {
          Ok((dyn_import_id, module_id)) => {
            self.dynamic_import_resolve(dyn_import_id, module_id);
          }
          Err((dyn_import_id, exception)) => {
            self.dynamic_import_reject(dyn_import_id, exception);
          }
        }
      }
    }
    self.state.borrow_mut().pending_dyn_mod_evaluate = still_pending;
    resolved_any
  }

  /// Asynchronously load specified module and all of its dependencies.
  ///
  /// The module will be marked as "main", and because of that
  /// "import.meta.main" will return true when checked inside that module.
  ///
  /// User must call [`JsRuntime::mod_evaluate`] with returned `ModuleId`
  /// manually after load is finished.
  pub async fn load_main_module(
    &mut self,
    specifier: &ModuleSpecifier,
    code: Option<ModuleCode>,
  ) -> Result<ModuleId, Error> {
    let module_map_rc = Self::module_map(self.v8_isolate());
    if let Some(code) = code {
      let specifier = specifier.as_str().to_owned().into();
      let scope = &mut self.handle_scope();
      // true for main module
      module_map_rc
        .borrow_mut()
        .new_es_module(scope, true, specifier, code, false)
        .map_err(|e| match e {
          ModuleError::Exception(exception) => {
            let exception = v8::Local::new(scope, exception);
            exception_to_err_result::<()>(scope, exception, false).unwrap_err()
          }
          ModuleError::Other(error) => error,
        })?;
    }

    let mut load =
      ModuleMap::load_main(module_map_rc.clone(), &specifier).await?;

    while let Some(load_result) = load.next().await {
      let (request, info) = load_result?;
      let scope = &mut self.handle_scope();
      load.register_and_recurse(scope, &request, info).map_err(
        |e| match e {
          ModuleError::Exception(exception) => {
            let exception = v8::Local::new(scope, exception);
            exception_to_err_result::<()>(scope, exception, false).unwrap_err()
          }
          ModuleError::Other(error) => error,
        },
      )?;
    }

    let root_id = load.root_module_id.expect("Root module should be loaded");
    self.instantiate_module(root_id).map_err(|e| {
      let scope = &mut self.handle_scope();
      let exception = v8::Local::new(scope, e);
      exception_to_err_result::<()>(scope, exception, false).unwrap_err()
    })?;
    Ok(root_id)
  }

  /// Asynchronously load specified ES module and all of its dependencies.
  ///
  /// This method is meant to be used when loading some utility code that
  /// might be later imported by the main module (ie. an entry point module).
  ///
  /// User must call [`JsRuntime::mod_evaluate`] with returned `ModuleId`
  /// manually after load is finished.
  pub async fn load_side_module(
    &mut self,
    specifier: &ModuleSpecifier,
    code: Option<ModuleCode>,
  ) -> Result<ModuleId, Error> {
    let module_map_rc = Self::module_map(self.v8_isolate());
    if let Some(code) = code {
      let specifier = specifier.as_str().to_owned().into();
      let scope = &mut self.handle_scope();
      // false for side module (not main module)
      module_map_rc
        .borrow_mut()
        .new_es_module(scope, false, specifier, code, false)
        .map_err(|e| match e {
          ModuleError::Exception(exception) => {
            let exception = v8::Local::new(scope, exception);
            exception_to_err_result::<()>(scope, exception, false).unwrap_err()
          }
          ModuleError::Other(error) => error,
        })?;
    }

    let mut load =
      ModuleMap::load_side(module_map_rc.clone(), &specifier).await?;

    while let Some(load_result) = load.next().await {
      let (request, info) = load_result?;
      let scope = &mut self.handle_scope();
      load.register_and_recurse(scope, &request, info).map_err(
        |e| match e {
          ModuleError::Exception(exception) => {
            let exception = v8::Local::new(scope, exception);
            exception_to_err_result::<()>(scope, exception, false).unwrap_err()
          }
          ModuleError::Other(error) => error,
        },
      )?;
    }

    let root_id = load.root_module_id.expect("Root module should be loaded");
    self.instantiate_module(root_id).map_err(|e| {
      let scope = &mut self.handle_scope();
      let exception = v8::Local::new(scope, e);
      exception_to_err_result::<()>(scope, exception, false).unwrap_err()
    })?;
    Ok(root_id)
  }

  fn check_promise_rejections(&mut self) -> Result<(), Error> {
    let state = self.state.clone();
    let scope = &mut self.handle_scope();
    let state = state.borrow();
    for weak_context in &state.known_realms {
      if let Some(context) = weak_context.to_local(scope) {
        JsRealmLocal::new(context).check_promise_rejections(scope)?;
      }
    }
    Ok(())
  }

  // Polls pending ops and then runs `Deno.core.eventLoopTick` callback.
  fn do_js_event_loop_tick(&mut self, cx: &mut Context) -> Result<(), Error> {
    // We have a specialized implementation of this method for the common case
    // where there is only one realm.
    let num_realms = self.state.borrow().known_realms.len();
    if num_realms == 1 {
      return self.do_single_realm_js_event_loop_tick(cx);
    }

    // `responses_per_realm[idx]` is a vector containing the promise ID and
    // response for all promises in realm `self.state.known_realms[idx]`.
    let mut responses_per_realm: Vec<Vec<(PromiseId, OpResult)>> =
      (0..num_realms).map(|_| vec![]).collect();

    // Now handle actual ops.
    {
      let mut state = self.state.borrow_mut();
      state.have_unpolled_ops = false;

      while let Poll::Ready(Some(item)) = state.pending_ops.poll_next_unpin(cx)
      {
        let (realm_idx, promise_id, _, resp) = item;
        responses_per_realm[realm_idx as usize].push((promise_id, resp));
      }
    }

    // Handle responses for each realm.
    let isolate = self.v8_isolate.as_mut().unwrap();
    for (realm_idx, responses) in responses_per_realm.into_iter().enumerate() {
      let realm = {
        let context = self.state.borrow().known_realms[realm_idx]
          .to_global(isolate)
          .unwrap();
        JsRealm::new(context)
      };
      let context_state_rc = realm.state(isolate);
      let mut context_state = context_state_rc.borrow_mut();
      let scope = &mut realm.handle_scope(isolate);

      // We return async responses to JS in unbounded batches (may change),
      // each batch is a flat vector of tuples:
      // `[promise_id1, op_result1, promise_id2, op_result2, ...]`
      // promise_id is a simple integer, op_result is an ops::OpResult
      // which contains a value OR an error, encoded as a tuple.
      // This batch is received in JS via the special `arguments` variable
      // and then each tuple is used to resolve or reject promises
      //
      // This can handle 15 promises futures in a single batch without heap
      // allocations.
      let mut args: SmallVec<[v8::Local<v8::Value>; 32]> =
        SmallVec::with_capacity(responses.len() * 2 + 2);

      for (promise_id, mut resp) in responses {
        context_state.unrefed_ops.remove(&promise_id);
        args.push(v8::Integer::new(scope, promise_id).into());
        args.push(match resp.to_v8(scope) {
          Ok(v) => v,
          Err(e) => OpResult::Err(OpError::new(&|_| "TypeError", e.into()))
            .to_v8(scope)
            .unwrap(),
        });
      }

      let has_tick_scheduled =
        v8::Boolean::new(scope, self.state.borrow().has_tick_scheduled);
      args.push(has_tick_scheduled.into());

      let js_event_loop_tick_cb_handle =
        context_state.js_event_loop_tick_cb.clone().unwrap();
      let tc_scope = &mut v8::TryCatch::new(scope);
      let js_event_loop_tick_cb = js_event_loop_tick_cb_handle.open(tc_scope);
      let this = v8::undefined(tc_scope).into();
      drop(context_state);
      js_event_loop_tick_cb.call(tc_scope, this, args.as_slice());

      if let Some(exception) = tc_scope.exception() {
        // TODO(@andreubotella): Returning here can cause async ops in other
        // realms to never resolve.
        return exception_to_err_result(tc_scope, exception, false);
      }

      if tc_scope.has_terminated() || tc_scope.is_execution_terminating() {
        return Ok(());
      }
    }

    Ok(())
  }

  fn do_single_realm_js_event_loop_tick(
    &mut self,
    cx: &mut Context,
  ) -> Result<(), Error> {
    let isolate = self.v8_isolate.as_mut().unwrap();
    let scope = &mut self
      .state
      .borrow()
      .global_realm
      .as_ref()
      .unwrap()
      .handle_scope(isolate);

    // We return async responses to JS in unbounded batches (may change),
    // each batch is a flat vector of tuples:
    // `[promise_id1, op_result1, promise_id2, op_result2, ...]`
    // promise_id is a simple integer, op_result is an ops::OpResult
    // which contains a value OR an error, encoded as a tuple.
    // This batch is received in JS via the special `arguments` variable
    // and then each tuple is used to resolve or reject promises
    //
    // This can handle 15 promises futures in a single batch without heap
    // allocations.
    let mut args: SmallVec<[v8::Local<v8::Value>; 32]> = SmallVec::new();

    // Now handle actual ops.
    {
      let mut state = self.state.borrow_mut();
      state.have_unpolled_ops = false;

      let realm_state_rc = state.global_realm.as_ref().unwrap().state(scope);
      let mut realm_state = realm_state_rc.borrow_mut();

      while let Poll::Ready(Some(item)) = state.pending_ops.poll_next_unpin(cx)
      {
        let (realm_idx, promise_id, _, mut resp) = item;
        debug_assert_eq!(
          state.known_realms[realm_idx as usize],
          state.global_realm.as_ref().unwrap().context()
        );
        realm_state.unrefed_ops.remove(&promise_id);
        args.push(v8::Integer::new(scope, promise_id).into());
        args.push(match resp.to_v8(scope) {
          Ok(v) => v,
          Err(e) => OpResult::Err(OpError::new(&|_| "TypeError", e.into()))
            .to_v8(scope)
            .unwrap(),
        });
      }
    }

    let has_tick_scheduled =
      v8::Boolean::new(scope, self.state.borrow().has_tick_scheduled);
    args.push(has_tick_scheduled.into());

    let js_event_loop_tick_cb_handle = {
      let state = self.state.borrow_mut();
      let realm_state_rc = state.global_realm.as_ref().unwrap().state(scope);
      let handle = realm_state_rc
        .borrow()
        .js_event_loop_tick_cb
        .clone()
        .unwrap();
      handle
    };
    let tc_scope = &mut v8::TryCatch::new(scope);
    let js_event_loop_tick_cb = js_event_loop_tick_cb_handle.open(tc_scope);
    let this = v8::undefined(tc_scope).into();
    js_event_loop_tick_cb.call(tc_scope, this, args.as_slice());

    if let Some(exception) = tc_scope.exception() {
      return exception_to_err_result(tc_scope, exception, false);
    }

    if tc_scope.has_terminated() || tc_scope.is_execution_terminating() {
      return Ok(());
    }

    Ok(())
  }
}

#[inline]
pub fn queue_fast_async_op<R: serde::Serialize + 'static>(
  ctx: &OpCtx,
  promise_id: PromiseId,
  op: impl Future<Output = Result<R, Error>> + 'static,
) {
  let runtime_state = match ctx.runtime_state.upgrade() {
    Some(rc_state) => rc_state,
    // atleast 1 Rc is held by the JsRuntime.
    None => unreachable!(),
  };
  let get_class = {
    let state = RefCell::borrow(&ctx.state);
    state.get_error_class_fn
  };
  let fut = op
    .map(|result| crate::_ops::to_op_result(get_class, result))
    .boxed_local();
  let mut state = runtime_state.borrow_mut();
  state
    .pending_ops
    .push(OpCall::pending(ctx, promise_id, fut));
  state.have_unpolled_ops = true;
}

#[inline]
pub fn map_async_op1<R: serde::Serialize + 'static>(
  ctx: &OpCtx,
  op: impl Future<Output = Result<R, Error>> + 'static,
) -> MaybeDone<Pin<Box<dyn Future<Output = OpResult>>>> {
  let get_class = {
    let state = RefCell::borrow(&ctx.state);
    state.get_error_class_fn
  };

  let fut = op
    .map(|result| crate::_ops::to_op_result(get_class, result))
    .boxed_local();
  MaybeDone::Future(fut)
}

#[inline]
pub fn map_async_op2<R: serde::Serialize + 'static>(
  op: impl Future<Output = R> + 'static,
) -> MaybeDone<Pin<Box<dyn Future<Output = OpResult>>>> {
  let fut = op.map(|result| OpResult::Ok(result.into())).boxed_local();
  MaybeDone::Future(fut)
}

#[inline]
pub fn map_async_op3<R: serde::Serialize + 'static>(
  ctx: &OpCtx,
  op: Result<impl Future<Output = Result<R, Error>> + 'static, Error>,
) -> MaybeDone<Pin<Box<dyn Future<Output = OpResult>>>> {
  let get_class = {
    let state = RefCell::borrow(&ctx.state);
    state.get_error_class_fn
  };

  match op {
    Err(err) => MaybeDone::Done(OpResult::Err(OpError::new(get_class, err))),
    Ok(fut) => MaybeDone::Future(
      fut
        .map(|result| crate::_ops::to_op_result(get_class, result))
        .boxed_local(),
    ),
  }
}

#[inline]
pub fn map_async_op4<R: serde::Serialize + 'static>(
  ctx: &OpCtx,
  op: Result<impl Future<Output = R> + 'static, Error>,
) -> MaybeDone<Pin<Box<dyn Future<Output = OpResult>>>> {
  let get_class = {
    let state = RefCell::borrow(&ctx.state);
    state.get_error_class_fn
  };

  match op {
    Err(err) => MaybeDone::Done(OpResult::Err(OpError::new(get_class, err))),
    Ok(fut) => MaybeDone::Future(
      fut.map(|result| OpResult::Ok(result.into())).boxed_local(),
    ),
  }
}

pub fn queue_async_op<'s>(
  ctx: &OpCtx,
  scope: &'s mut v8::HandleScope,
  deferred: bool,
  promise_id: PromiseId,
  mut op: MaybeDone<Pin<Box<dyn Future<Output = OpResult>>>>,
) -> Option<v8::Local<'s, v8::Value>> {
  let runtime_state = match ctx.runtime_state.upgrade() {
    Some(rc_state) => rc_state,
    // atleast 1 Rc is held by the JsRuntime.
    None => unreachable!(),
  };

  // An op's realm (as given by `OpCtx::realm_idx`) must match the realm in
  // which it is invoked. Otherwise, we might have cross-realm object exposure.
  // core doesn't currently support such exposure, even though embedders
  // can cause them, so we panic in debug mode (since the check is expensive).
  debug_assert_eq!(
    runtime_state.borrow().known_realms[ctx.realm_idx as usize].to_local(scope),
    Some(scope.get_current_context())
  );

  // All ops are polled immediately
  let waker = noop_waker();
  let mut cx = Context::from_waker(&waker);

  // Note that MaybeDone returns () from the future
  let op_call = match op.poll_unpin(&mut cx) {
    Poll::Pending => {
      let MaybeDone::Future(fut) = op else {
        unreachable!()
      };
      OpCall::pending(ctx, promise_id, fut)
    }
    Poll::Ready(_) => {
      let mut op_result = Pin::new(&mut op).take_output().unwrap();
      // If the op is ready and is not marked as deferred we can immediately return
      // the result.
      if !deferred {
        return Some(op_result.to_v8(scope).unwrap());
      }

      OpCall::ready(ctx, promise_id, op_result)
    }
  };

  // Otherwise we will push it to the `pending_ops` and let it be polled again
  // or resolved on the next tick of the event loop.
  let mut state = runtime_state.borrow_mut();
  state.pending_ops.push(op_call);
  state.have_unpolled_ops = true;
  None
}
