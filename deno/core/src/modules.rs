// Copyright 2018-2023 the Deno authors. All rights reserved. MIT license.

use crate::bindings;
use crate::error::generic_error;
use crate::extensions::ExtensionFileSource;
use crate::fast_string::FastString;
use crate::module_specifier::ModuleSpecifier;
use crate::resolve_url;
use crate::snapshot_util::SnapshottedData;
use crate::Extension;
use crate::JsRuntime;
use crate::OpState;
use anyhow::Error;
use core::panic;
use futures::future::FutureExt;
use futures::stream::FuturesUnordered;
use futures::stream::Stream;
use futures::stream::StreamFuture;
use futures::stream::TryStreamExt;
use log::debug;
use serde::Deserialize;
use serde::Serialize;
use std::cell::RefCell;
use std::collections::HashMap;
use std::collections::HashSet;
use std::collections::VecDeque;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::task::Context;
use std::task::Poll;

pub type ModuleId = usize;
pub(crate) type ModuleLoadId = i32;
pub type ModuleCode = FastString;
pub type ModuleName = FastString;

pub const BOM_CHAR: &[u8] = &[0xef, 0xbb, 0xbf];

/// Strips the byte order mark from the provided text if it exists.
fn strip_bom(source_code: &[u8]) -> &[u8] {
  if source_code.starts_with(BOM_CHAR) {
    &source_code[BOM_CHAR.len()..]
  } else {
    source_code
  }
}

const SUPPORTED_TYPE_ASSERTIONS: &[&str] = &["json"];

/// Throws V8 exception if assertions are invalid
pub(crate) fn validate_import_assertions(
  scope: &mut v8::HandleScope,
  assertions: &HashMap<String, String>,
) {
  for (key, value) in assertions {
    if key == "type" && !SUPPORTED_TYPE_ASSERTIONS.contains(&value.as_str()) {
      let message = v8::String::new(
        scope,
        &format!("\"{value}\" is not a valid module type."),
      )
      .unwrap();
      let exception = v8::Exception::type_error(scope, message);
      scope.throw_exception(exception);
      return;
    }
  }
}

#[derive(Debug)]
pub(crate) enum ImportAssertionsKind {
  StaticImport,
  DynamicImport,
}

pub(crate) fn parse_import_assertions(
  scope: &mut v8::HandleScope,
  import_assertions: v8::Local<v8::FixedArray>,
  kind: ImportAssertionsKind,
) -> HashMap<String, String> {
  let mut assertions: HashMap<String, String> = HashMap::default();

  let assertions_per_line = match kind {
    // For static imports, assertions are triples of (keyword, value and source offset)
    // Also used in `module_resolve_callback`.
    ImportAssertionsKind::StaticImport => 3,
    // For dynamic imports, assertions are tuples of (keyword, value)
    ImportAssertionsKind::DynamicImport => 2,
  };
  assert_eq!(import_assertions.length() % assertions_per_line, 0);
  let no_of_assertions = import_assertions.length() / assertions_per_line;

  for i in 0..no_of_assertions {
    let assert_key = import_assertions
      .get(scope, assertions_per_line * i)
      .unwrap();
    let assert_key_val = v8::Local::<v8::Value>::try_from(assert_key).unwrap();
    let assert_value = import_assertions
      .get(scope, (assertions_per_line * i) + 1)
      .unwrap();
    let assert_value_val =
      v8::Local::<v8::Value>::try_from(assert_value).unwrap();
    assertions.insert(
      assert_key_val.to_rust_string_lossy(scope),
      assert_value_val.to_rust_string_lossy(scope),
    );
  }

  assertions
}

pub(crate) fn get_asserted_module_type_from_assertions(
  assertions: &HashMap<String, String>,
) -> AssertedModuleType {
  assertions
    .get("type")
    .map(|ty| {
      if ty == "json" {
        AssertedModuleType::Json
      } else {
        AssertedModuleType::JavaScriptOrWasm
      }
    })
    .unwrap_or(AssertedModuleType::JavaScriptOrWasm)
}

// Clippy thinks the return value doesn't need to be an Option, it's unaware
// of the mapping that MapFnFrom<F> does for ResolveModuleCallback.
#[allow(clippy::unnecessary_wraps)]
fn json_module_evaluation_steps<'a>(
  context: v8::Local<'a, v8::Context>,
  module: v8::Local<v8::Module>,
) -> Option<v8::Local<'a, v8::Value>> {
  // SAFETY: `CallbackScope` can be safely constructed from `Local<Context>`
  let scope = &mut unsafe { v8::CallbackScope::new(context) };
  let tc_scope = &mut v8::TryCatch::new(scope);
  let module_map = JsRuntime::module_map(tc_scope);

  let handle = v8::Global::<v8::Module>::new(tc_scope, module);
  let value_handle = module_map
    .borrow_mut()
    .json_value_store
    .remove(&handle)
    .unwrap();
  let value_local = v8::Local::new(tc_scope, value_handle);

  let name = v8::String::new(tc_scope, "default").unwrap();
  // This should never fail
  assert!(
    module.set_synthetic_module_export(tc_scope, name, value_local)
      == Some(true)
  );
  assert!(!tc_scope.has_caught());

  // Since TLA is active we need to return a promise.
  let resolver = v8::PromiseResolver::new(tc_scope).unwrap();
  let undefined = v8::undefined(tc_scope);
  resolver.resolve(tc_scope, undefined.into());
  Some(resolver.get_promise(tc_scope).into())
}

/// A type of module to be executed.
///
/// For non-`JavaScript` modules, this value doesn't tell
/// how to interpret the module; it is only used to validate
/// the module against an import assertion (if one is present
/// in the import statement).
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[repr(u32)]
pub enum ModuleType {
  JavaScript,
  Json,
}

impl std::fmt::Display for ModuleType {
  fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
    match self {
      Self::JavaScript => write!(f, "JavaScript"),
      Self::Json => write!(f, "JSON"),
    }
  }
}

/// EsModule source code that will be loaded into V8.
///
/// Users can implement `Into<ModuleInfo>` for different file types that
/// can be transpiled to valid EsModule.
///
/// Found module URL might be different from specified URL
/// used for loading due to redirections (like HTTP 303).
/// Eg. Both "`https://example.com/a.ts`" and
/// "`https://example.com/b.ts`" may point to "`https://example.com/c.ts`"
/// By keeping track of specified and found URL we can alias modules and avoid
/// recompiling the same code 3 times.
// TODO(bartlomieju): I have a strong opinion we should store all redirects
// that happened; not only first and final target. It would simplify a lot
// of things throughout the codebase otherwise we may end up requesting
// intermediate redirects from file loader.
// NOTE: This should _not_ be made #[derive(Clone)] unless we take some precautions to avoid excessive string copying.
#[derive(Debug)]
pub struct ModuleSource {
  pub code: ModuleCode,
  pub module_type: ModuleType,
  module_url_specified: ModuleName,
  /// If the module was found somewhere other than the specified address, this will be [`Some`].
  module_url_found: Option<ModuleName>,
}

impl ModuleSource {
  /// Create a [`ModuleSource`] without a redirect.
  pub fn new(
    module_type: impl Into<ModuleType>,
    code: ModuleCode,
    specifier: &ModuleSpecifier,
  ) -> Self {
    let module_url_specified = specifier.as_ref().to_owned().into();
    Self {
      code,
      module_type: module_type.into(),
      module_url_specified,
      module_url_found: None,
    }
  }

  /// Create a [`ModuleSource`] with a potential redirect. If the `specifier_found` parameter is the same as the
  /// specifier, the code behaves the same was as `ModuleSource::new`.
  pub fn new_with_redirect(
    module_type: impl Into<ModuleType>,
    code: ModuleCode,
    specifier: &ModuleSpecifier,
    specifier_found: &ModuleSpecifier,
  ) -> Self {
    let module_url_found = if specifier == specifier_found {
      None
    } else {
      Some(specifier_found.as_ref().to_owned().into())
    };
    let module_url_specified = specifier.as_ref().to_owned().into();
    Self {
      code,
      module_type: module_type.into(),
      module_url_specified,
      module_url_found,
    }
  }

  #[cfg(test)]
  pub fn for_test(code: &'static str, file: impl AsRef<str>) -> Self {
    Self {
      code: ModuleCode::from_static(code),
      module_type: ModuleType::JavaScript,
      module_url_specified: file.as_ref().to_owned().into(),
      module_url_found: None,
    }
  }

  /// If the `found` parameter is the same as the `specified` parameter, the code behaves the same was as `ModuleSource::for_test`.
  #[cfg(test)]
  pub fn for_test_with_redirect(
    code: &'static str,
    specified: impl AsRef<str>,
    found: impl AsRef<str>,
  ) -> Self {
    let specified = specified.as_ref().to_string();
    let found = found.as_ref().to_string();
    let found = if found == specified {
      None
    } else {
      Some(found.into())
    };
    Self {
      code: ModuleCode::from_static(code),
      module_type: ModuleType::JavaScript,
      module_url_specified: specified.into(),
      module_url_found: found,
    }
  }
}

pub(crate) type PrepareLoadFuture =
  dyn Future<Output = (ModuleLoadId, Result<RecursiveModuleLoad, Error>)>;
pub type ModuleSourceFuture = dyn Future<Output = Result<ModuleSource, Error>>;

type ModuleLoadFuture =
  dyn Future<Output = Result<(ModuleRequest, ModuleSource), Error>>;

#[derive(Debug, PartialEq, Eq)]
pub enum ResolutionKind {
  /// This kind is used in only one situation: when a module is loaded via
  /// `JsRuntime::load_main_module` and is the top-level module, ie. the one
  /// passed as an argument to `JsRuntime::load_main_module`.
  MainModule,
  /// This kind is returned for all other modules during module load, that are
  /// static imports.
  Import,
  /// This kind is returned for all modules that are loaded as a result of a
  /// call to `import()` API (ie. top-level module as well as all its
  /// dependencies, and any other `import()` calls from that load).
  DynamicImport,
}

pub trait ModuleLoader {
  /// Returns an absolute URL.
  /// When implementing an spec-complaint VM, this should be exactly the
  /// algorithm described here:
  /// <https://html.spec.whatwg.org/multipage/webappapis.html#resolve-a-module-specifier>
  ///
  /// `is_main` can be used to resolve from current working directory or
  /// apply import map for child imports.
  ///
  /// `is_dyn_import` can be used to check permissions or deny
  /// dynamic imports altogether.
  fn resolve(
    &self,
    specifier: &str,
    referrer: &str,
    kind: ResolutionKind,
  ) -> Result<ModuleSpecifier, Error>;

  /// Given ModuleSpecifier, load its source code.
  ///
  /// `is_dyn_import` can be used to check permissions or deny
  /// dynamic imports altogether.
  fn load(
    &self,
    module_specifier: &ModuleSpecifier,
    maybe_referrer: Option<&ModuleSpecifier>,
    is_dyn_import: bool,
  ) -> Pin<Box<ModuleSourceFuture>>;

  /// This hook can be used by implementors to do some preparation
  /// work before starting loading of modules.
  ///
  /// For example implementor might download multiple modules in
  /// parallel and transpile them to final JS sources before
  /// yielding control back to the runtime.
  ///
  /// It's not required to implement this method.
  fn prepare_load(
    &self,
    _op_state: Rc<RefCell<OpState>>,
    _module_specifier: &ModuleSpecifier,
    _maybe_referrer: Option<String>,
    _is_dyn_import: bool,
  ) -> Pin<Box<dyn Future<Output = Result<(), Error>>>> {
    async { Ok(()) }.boxed_local()
  }
}

/// Placeholder structure used when creating
/// a runtime that doesn't support module loading.
pub struct NoopModuleLoader;

impl ModuleLoader for NoopModuleLoader {
  fn resolve(
    &self,
    specifier: &str,
    referrer: &str,
    _kind: ResolutionKind,
  ) -> Result<ModuleSpecifier, Error> {
    Err(generic_error(
      format!("Module loading is not supported; attempted to resolve: \"{specifier}\" from \"{referrer}\"")
    ))
  }

  fn load(
    &self,
    module_specifier: &ModuleSpecifier,
    maybe_referrer: Option<&ModuleSpecifier>,
    _is_dyn_import: bool,
  ) -> Pin<Box<ModuleSourceFuture>> {
    let err = generic_error(
      format!(
        "Module loading is not supported; attempted to load: \"{module_specifier}\" from \"{maybe_referrer:?}\"",
      )
    );
    async move { Err(err) }.boxed_local()
  }
}

/// Function that can be passed to the `ExtModuleLoader` that allows to
/// transpile sources before passing to V8.
pub type ExtModuleLoaderCb =
  Box<dyn Fn(&ExtensionFileSource) -> Result<ModuleCode, Error>>;

pub struct ExtModuleLoader {
  module_loader: Rc<dyn ModuleLoader>,
  extensions: Rc<RefCell<Vec<Extension>>>,
  ext_resolution_allowed: RefCell<bool>,
  used_esm_sources: RefCell<HashMap<String, bool>>,
  maybe_load_callback: Option<ExtModuleLoaderCb>,
}

impl Default for ExtModuleLoader {
  fn default() -> Self {
    Self {
      module_loader: Rc::new(NoopModuleLoader),
      extensions: Default::default(),
      ext_resolution_allowed: Default::default(),
      used_esm_sources: RefCell::new(HashMap::default()),
      maybe_load_callback: None,
    }
  }
}

impl ExtModuleLoader {
  pub fn new(
    module_loader: Option<Rc<dyn ModuleLoader>>,
    extensions: Rc<RefCell<Vec<Extension>>>,
    maybe_load_callback: Option<ExtModuleLoaderCb>,
  ) -> Self {
    let used_esm_sources: HashMap<String, bool> = extensions
      .borrow()
      .iter()
      .flat_map(|e| e.get_esm_sources())
      .flatten()
      .map(|file_source| (file_source.specifier.to_string(), false))
      .collect();

    ExtModuleLoader {
      module_loader: module_loader.unwrap_or_else(|| Rc::new(NoopModuleLoader)),
      extensions,
      ext_resolution_allowed: Default::default(),
      used_esm_sources: RefCell::new(used_esm_sources),
      maybe_load_callback,
    }
  }

  pub fn resolve(
    &self,
    specifier: &str,
    referrer: &str,
    kind: ResolutionKind,
  ) -> Result<ModuleSpecifier, Error> {
    if specifier.starts_with("ext:") {
      if !referrer.starts_with("ext:") && referrer != "."
        || !*self.ext_resolution_allowed.borrow()
      {
        return Err(generic_error(
          "Cannot load extension module from external code",
        ));
      }
      return Ok(ModuleSpecifier::parse(specifier)?);
    }
    self.module_loader.resolve(specifier, referrer, kind)
  }

  pub fn load(
    &self,
    module_specifier: &ModuleSpecifier,
    maybe_referrer: Option<&ModuleSpecifier>,
    is_dyn_import: bool,
  ) -> Pin<Box<ModuleSourceFuture>> {
    if module_specifier.scheme() != "ext" {
      return self.module_loader.load(
        module_specifier,
        maybe_referrer,
        is_dyn_import,
      );
    }

    let specifier = module_specifier.to_string();
    let extensions = self.extensions.borrow();
    let maybe_file_source = extensions
      .iter()
      .find_map(|e| e.find_esm(module_specifier.as_str()));

    if let Some(file_source) = maybe_file_source {
      {
        let mut used_esm_sources = self.used_esm_sources.borrow_mut();
        let used = used_esm_sources.get_mut(file_source.specifier).unwrap();
        *used = true;
      }

      let result = if let Some(load_callback) = &self.maybe_load_callback {
        load_callback(file_source)
      } else {
        file_source.load()
      };

      match result {
        Ok(code) => {
          let res =
            ModuleSource::new(ModuleType::JavaScript, code, module_specifier);
          return futures::future::ok(res).boxed_local();
        }
        Err(err) => return futures::future::err(err).boxed_local(),
      }
    }

    async move {
      Err(generic_error(format!(
        "Cannot find extension module source for specifier {specifier}"
      )))
    }
    .boxed_local()
  }

  pub fn prepare_load(
    &self,
    op_state: Rc<RefCell<OpState>>,
    module_specifier: &ModuleSpecifier,
    maybe_referrer: Option<String>,
    is_dyn_import: bool,
  ) -> Pin<Box<dyn Future<Output = Result<(), Error>>>> {
    if module_specifier.scheme() == "ext" {
      return async { Ok(()) }.boxed_local();
    }

    self.module_loader.prepare_load(
      op_state,
      module_specifier,
      maybe_referrer,
      is_dyn_import,
    )
  }

  pub fn allow_ext_resolution(&self) {
    *self.ext_resolution_allowed.borrow_mut() = true;
  }

  pub fn disallow_ext_resolution(&self) {
    *self.ext_resolution_allowed.borrow_mut() = false;
  }
}

impl Drop for ExtModuleLoader {
  fn drop(&mut self) {
    let used_esm_sources = self.used_esm_sources.get_mut();
    let unused_modules: Vec<_> = used_esm_sources
      .iter()
      .filter(|(_s, v)| !*v)
      .map(|(s, _)| s)
      .collect();

    if !unused_modules.is_empty() {
      let mut msg =
        "Following modules were passed to ExtModuleLoader but never used:\n"
          .to_string();
      for m in unused_modules {
        msg.push_str("  - ");
        msg.push_str(m);
        msg.push('\n');
      }
      panic!("{}", msg);
    }
  }
}

/// Describes the entrypoint of a recursive module load.
#[derive(Debug)]
enum LoadInit {
  /// Main module specifier.
  Main(String),
  /// Module specifier for side module.
  Side(String),
  /// Dynamic import specifier with referrer and expected
  /// module type (which is determined by import assertion).
  DynamicImport(String, String, AssertedModuleType),
}

#[derive(Debug, Eq, PartialEq)]
pub enum LoadState {
  Init,
  LoadingRoot,
  LoadingImports,
  Done,
}

/// This future is used to implement parallel async module loading.
pub(crate) struct RecursiveModuleLoad {
  pub id: ModuleLoadId,
  pub root_module_id: Option<ModuleId>,
  init: LoadInit,
  root_asserted_module_type: Option<AssertedModuleType>,
  root_module_type: Option<ModuleType>,
  state: LoadState,
  module_map_rc: Rc<RefCell<ModuleMap>>,
  pending: FuturesUnordered<Pin<Box<ModuleLoadFuture>>>,
  visited: HashSet<ModuleRequest>,
  // These three fields are copied from `module_map_rc`, but they are cloned
  // ahead of time to avoid already-borrowed errors.
  op_state: Rc<RefCell<OpState>>,
  loader: Rc<ExtModuleLoader>,
}

impl RecursiveModuleLoad {
  /// Starts a new asynchronous load of the module graph for given specifier.
  ///
  /// The module corresponding for the given `specifier` will be marked as
  // "the main module" (`import.meta.main` will return `true` for this module).
  fn main(specifier: &str, module_map_rc: Rc<RefCell<ModuleMap>>) -> Self {
    Self::new(LoadInit::Main(specifier.to_string()), module_map_rc)
  }

  /// Starts a new asynchronous load of the module graph for given specifier.
  fn side(specifier: &str, module_map_rc: Rc<RefCell<ModuleMap>>) -> Self {
    Self::new(LoadInit::Side(specifier.to_string()), module_map_rc)
  }

  /// Starts a new asynchronous load of the module graph for given specifier
  /// that was imported using `import()`.
  fn dynamic_import(
    specifier: &str,
    referrer: &str,
    asserted_module_type: AssertedModuleType,
    module_map_rc: Rc<RefCell<ModuleMap>>,
  ) -> Self {
    Self::new(
      LoadInit::DynamicImport(
        specifier.to_string(),
        referrer.to_string(),
        asserted_module_type,
      ),
      module_map_rc,
    )
  }

  fn new(init: LoadInit, module_map_rc: Rc<RefCell<ModuleMap>>) -> Self {
    let id = {
      let mut module_map = module_map_rc.borrow_mut();
      let id = module_map.next_load_id;
      module_map.next_load_id += 1;
      id
    };
    let op_state = module_map_rc.borrow().op_state.clone();
    let loader = module_map_rc.borrow().loader.clone();
    let asserted_module_type = match init {
      LoadInit::DynamicImport(_, _, module_type) => module_type,
      _ => AssertedModuleType::JavaScriptOrWasm,
    };
    let mut load = Self {
      id,
      root_module_id: None,
      root_asserted_module_type: None,
      root_module_type: None,
      init,
      state: LoadState::Init,
      module_map_rc: module_map_rc.clone(),
      op_state,
      loader,
      pending: FuturesUnordered::new(),
      visited: HashSet::new(),
    };
    // FIXME(bartlomieju): this seems fishy
    // Ignore the error here, let it be hit in `Stream::poll_next()`.
    if let Ok(root_specifier) = load.resolve_root() {
      if let Some(module_id) = module_map_rc
        .borrow()
        .get_id(root_specifier, asserted_module_type)
      {
        load.root_module_id = Some(module_id);
        load.root_asserted_module_type = Some(asserted_module_type);
        load.root_module_type = Some(
          module_map_rc
            .borrow()
            .get_info_by_id(module_id)
            .unwrap()
            .module_type,
        );
      }
    }
    load
  }

  fn resolve_root(&self) -> Result<ModuleSpecifier, Error> {
    match self.init {
      LoadInit::Main(ref specifier) => {
        self
          .loader
          .resolve(specifier, ".", ResolutionKind::MainModule)
      }
      LoadInit::Side(ref specifier) => {
        self.loader.resolve(specifier, ".", ResolutionKind::Import)
      }
      LoadInit::DynamicImport(ref specifier, ref referrer, _) => self
        .loader
        .resolve(specifier, referrer, ResolutionKind::DynamicImport),
    }
  }

  async fn prepare(&self) -> Result<(), Error> {
    let op_state = self.op_state.clone();

    let (module_specifier, maybe_referrer) = match self.init {
      LoadInit::Main(ref specifier) => {
        let spec =
          self
            .loader
            .resolve(specifier, ".", ResolutionKind::MainModule)?;
        (spec, None)
      }
      LoadInit::Side(ref specifier) => {
        let spec =
          self
            .loader
            .resolve(specifier, ".", ResolutionKind::Import)?;
        (spec, None)
      }
      LoadInit::DynamicImport(ref specifier, ref referrer, _) => {
        let spec = self.loader.resolve(
          specifier,
          referrer,
          ResolutionKind::DynamicImport,
        )?;
        (spec, Some(referrer.to_string()))
      }
    };

    self
      .loader
      .prepare_load(
        op_state,
        &module_specifier,
        maybe_referrer,
        self.is_dynamic_import(),
      )
      .await
  }

  fn is_currently_loading_main_module(&self) -> bool {
    !self.is_dynamic_import()
      && matches!(self.init, LoadInit::Main(..))
      && self.state == LoadState::LoadingRoot
  }

  fn is_dynamic_import(&self) -> bool {
    matches!(self.init, LoadInit::DynamicImport(..))
  }

  pub(crate) fn register_and_recurse(
    &mut self,
    scope: &mut v8::HandleScope,
    module_request: &ModuleRequest,
    module_source: ModuleSource,
  ) -> Result<(), ModuleError> {
    let expected_asserted_module_type = module_source.module_type.into();
    let module_url_found = module_source.module_url_found;
    let module_url_specified = module_source.module_url_specified;

    if module_request.asserted_module_type != expected_asserted_module_type {
      return Err(ModuleError::Other(generic_error(format!(
        "Expected a \"{}\" module but loaded a \"{}\" module.",
        module_request.asserted_module_type, module_source.module_type,
      ))));
    }

    // Register the module in the module map unless it's already there. If the
    // specified URL and the "true" URL are different, register the alias.
    let module_url_found = if let Some(module_url_found) = module_url_found {
      let (module_url_found1, module_url_found2) =
        module_url_found.into_cheap_copy();
      self.module_map_rc.borrow_mut().alias(
        module_url_specified,
        expected_asserted_module_type,
        module_url_found1,
      );
      module_url_found2
    } else {
      module_url_specified
    };

    let maybe_module_id = self
      .module_map_rc
      .borrow()
      .get_id(&module_url_found, expected_asserted_module_type);
    let module_id = match maybe_module_id {
      Some(id) => {
        debug!(
          "Already-registered module fetched again: {:?}",
          module_url_found
        );
        id
      }
      None => match module_source.module_type {
        ModuleType::JavaScript => {
          self.module_map_rc.borrow_mut().new_es_module(
            scope,
            self.is_currently_loading_main_module(),
            module_url_found,
            module_source.code,
            self.is_dynamic_import(),
          )?
        }
        ModuleType::Json => self.module_map_rc.borrow_mut().new_json_module(
          scope,
          module_url_found,
          module_source.code,
        )?,
      },
    };

    // Recurse the module's imports. There are two cases for each import:
    // 1. If the module is not in the module map, start a new load for it in
    //    `self.pending`. The result of that load should eventually be passed to
    //    this function for recursion.
    // 2. If the module is already in the module map, queue it up to be
    //    recursed synchronously here.
    // This robustly ensures that the whole graph is in the module map before
    // `LoadState::Done` is set.
    let mut already_registered = VecDeque::new();
    already_registered.push_back((module_id, module_request.clone()));
    self.visited.insert(module_request.clone());
    while let Some((module_id, module_request)) = already_registered.pop_front()
    {
      let referrer = ModuleSpecifier::parse(&module_request.specifier).unwrap();
      let imports = self
        .module_map_rc
        .borrow()
        .get_requested_modules(module_id)
        .unwrap()
        .clone();
      for module_request in imports {
        if !self.visited.contains(&module_request) {
          if let Some(module_id) = self.module_map_rc.borrow().get_id(
            module_request.specifier.as_str(),
            module_request.asserted_module_type,
          ) {
            already_registered.push_back((module_id, module_request.clone()));
          } else {
            let request = module_request.clone();
            let specifier =
              ModuleSpecifier::parse(&module_request.specifier).unwrap();
            let referrer = referrer.clone();
            let loader = self.loader.clone();
            let is_dynamic_import = self.is_dynamic_import();
            let fut = async move {
              let load_result = loader
                .load(&specifier, Some(&referrer), is_dynamic_import)
                .await;
              load_result.map(|s| (request, s))
            };
            self.pending.push(fut.boxed_local());
          }
          self.visited.insert(module_request);
        }
      }
    }

    // Update `self.state` however applicable.
    if self.state == LoadState::LoadingRoot {
      self.root_module_id = Some(module_id);
      self.root_asserted_module_type = Some(module_source.module_type.into());
      self.state = LoadState::LoadingImports;
    }
    if self.pending.is_empty() {
      self.state = LoadState::Done;
    }

    Ok(())
  }
}

impl Stream for RecursiveModuleLoad {
  type Item = Result<(ModuleRequest, ModuleSource), Error>;

  fn poll_next(
    self: Pin<&mut Self>,
    cx: &mut Context,
  ) -> Poll<Option<Self::Item>> {
    let inner = self.get_mut();
    // IMPORTANT: Do not borrow `inner.module_map_rc` here. It may not be
    // available.
    match inner.state {
      LoadState::Init => {
        let module_specifier = match inner.resolve_root() {
          Ok(url) => url,
          Err(error) => return Poll::Ready(Some(Err(error))),
        };
        let load_fut = if let Some(_module_id) = inner.root_module_id {
          // FIXME(bartlomieju): this is very bad
          // The root module is already in the module map.
          // TODO(nayeemrmn): In this case we would ideally skip to
          // `LoadState::LoadingImports` and synchronously recurse the imports
          // like the bottom of `RecursiveModuleLoad::register_and_recurse()`.
          // But the module map cannot be borrowed here. Instead fake a load
          // event so it gets passed to that function and recursed eventually.
          let asserted_module_type = inner.root_asserted_module_type.unwrap();
          let module_type = inner.root_module_type.unwrap();
          let module_request = ModuleRequest {
            specifier: module_specifier.to_string(),
            asserted_module_type,
          };
          // The code will be discarded, since this module is already in the
          // module map.
          let module_source = ModuleSource::new(
            module_type,
            Default::default(),
            &module_specifier,
          );
          futures::future::ok((module_request, module_source)).boxed()
        } else {
          let maybe_referrer = match inner.init {
            LoadInit::DynamicImport(_, ref referrer, _) => {
              resolve_url(referrer).ok()
            }
            _ => None,
          };
          let asserted_module_type = match inner.init {
            LoadInit::DynamicImport(_, _, module_type) => module_type,
            _ => AssertedModuleType::JavaScriptOrWasm,
          };
          let module_request = ModuleRequest {
            specifier: module_specifier.to_string(),
            asserted_module_type,
          };
          let loader = inner.loader.clone();
          let is_dynamic_import = inner.is_dynamic_import();
          async move {
            let result = loader
              .load(
                &module_specifier,
                maybe_referrer.as_ref(),
                is_dynamic_import,
              )
              .await;
            result.map(|s| (module_request, s))
          }
          .boxed_local()
        };
        inner.pending.push(load_fut);
        inner.state = LoadState::LoadingRoot;
        inner.try_poll_next_unpin(cx)
      }
      LoadState::LoadingRoot | LoadState::LoadingImports => {
        match inner.pending.try_poll_next_unpin(cx)? {
          Poll::Ready(None) => unreachable!(),
          Poll::Ready(Some(info)) => Poll::Ready(Some(Ok(info))),
          Poll::Pending => Poll::Pending,
        }
      }
      LoadState::Done => Poll::Ready(None),
    }
  }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[repr(u32)]
pub(crate) enum AssertedModuleType {
  JavaScriptOrWasm,
  Json,
}

impl From<ModuleType> for AssertedModuleType {
  fn from(module_type: ModuleType) -> AssertedModuleType {
    match module_type {
      ModuleType::JavaScript => AssertedModuleType::JavaScriptOrWasm,
      ModuleType::Json => AssertedModuleType::Json,
    }
  }
}

impl std::fmt::Display for AssertedModuleType {
  fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
    match self {
      Self::JavaScriptOrWasm => write!(f, "JavaScriptOrWasm"),
      Self::Json => write!(f, "JSON"),
    }
  }
}

/// Describes a request for a module as parsed from the source code.
/// Usually executable (`JavaScriptOrWasm`) is used, except when an
/// import assertions explicitly constrains an import to JSON, in
/// which case this will have a `AssertedModuleType::Json`.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub(crate) struct ModuleRequest {
  pub specifier: String,
  pub asserted_module_type: AssertedModuleType,
}

#[derive(Debug, PartialEq)]
pub(crate) struct ModuleInfo {
  #[allow(unused)]
  pub id: ModuleId,
  // Used in "bindings.rs" for "import.meta.main" property value.
  pub main: bool,
  pub name: ModuleName,
  pub requests: Vec<ModuleRequest>,
  pub module_type: ModuleType,
}

/// A symbolic module entity.
#[derive(Debug, PartialEq)]
pub(crate) enum SymbolicModule {
  /// This module is an alias to another module.
  /// This is useful such that multiple names could point to
  /// the same underlying module (particularly due to redirects).
  Alias(ModuleName),
  /// This module associates with a V8 module by id.
  Mod(ModuleId),
}

#[derive(Debug)]
pub(crate) enum ModuleError {
  Exception(v8::Global<v8::Value>),
  Other(Error),
}

/// A collection of JS modules.
pub(crate) struct ModuleMap {
  // Handling of specifiers and v8 objects
  pub handles: Vec<v8::Global<v8::Module>>,
  pub info: Vec<ModuleInfo>,
  pub(crate) by_name_js: HashMap<ModuleName, SymbolicModule>,
  pub(crate) by_name_json: HashMap<ModuleName, SymbolicModule>,
  pub(crate) next_load_id: ModuleLoadId,

  // Handling of futures for loading module sources
  pub loader: Rc<ExtModuleLoader>,
  op_state: Rc<RefCell<OpState>>,
  pub(crate) dynamic_import_map:
    HashMap<ModuleLoadId, v8::Global<v8::PromiseResolver>>,
  pub(crate) preparing_dynamic_imports:
    FuturesUnordered<Pin<Box<PrepareLoadFuture>>>,
  pub(crate) pending_dynamic_imports:
    FuturesUnordered<StreamFuture<RecursiveModuleLoad>>,

  // This store is used temporarly, to forward parsed JSON
  // value from `new_json_module` to `json_module_evaluation_steps`
  json_value_store: HashMap<v8::Global<v8::Module>, v8::Global<v8::Value>>,
}

impl ModuleMap {
  pub fn collect_modules(
    &self,
  ) -> Vec<(AssertedModuleType, &ModuleName, &SymbolicModule)> {
    let mut output = vec![];
    for module_type in [
      AssertedModuleType::JavaScriptOrWasm,
      AssertedModuleType::Json,
    ] {
      output.extend(
        self
          .by_name(module_type)
          .iter()
          .map(|x| (module_type, x.0, x.1)),
      )
    }
    output
  }

  pub fn serialize_for_snapshotting(
    &self,
    scope: &mut v8::HandleScope,
  ) -> SnapshottedData {
    let array = v8::Array::new(scope, 3);

    let next_load_id = v8::Integer::new(scope, self.next_load_id);
    array.set_index(scope, 0, next_load_id.into());

    let info_arr = v8::Array::new(scope, self.info.len() as i32);
    for (i, info) in self.info.iter().enumerate() {
      let module_info_arr = v8::Array::new(scope, 5);

      let id = v8::Integer::new(scope, info.id as i32);
      module_info_arr.set_index(scope, 0, id.into());

      let main = v8::Boolean::new(scope, info.main);
      module_info_arr.set_index(scope, 1, main.into());

      let name = info.name.v8(scope);
      module_info_arr.set_index(scope, 2, name.into());

      let array_len = 2 * info.requests.len() as i32;
      let requests_arr = v8::Array::new(scope, array_len);
      for (i, request) in info.requests.iter().enumerate() {
        let specifier = v8::String::new_from_one_byte(
          scope,
          request.specifier.as_bytes(),
          v8::NewStringType::Normal,
        )
        .unwrap();
        requests_arr.set_index(scope, 2 * i as u32, specifier.into());

        let asserted_module_type =
          v8::Integer::new(scope, request.asserted_module_type as i32);
        requests_arr.set_index(
          scope,
          (2 * i) as u32 + 1,
          asserted_module_type.into(),
        );
      }
      module_info_arr.set_index(scope, 3, requests_arr.into());

      let module_type = v8::Integer::new(scope, info.module_type as i32);
      module_info_arr.set_index(scope, 4, module_type.into());

      info_arr.set_index(scope, i as u32, module_info_arr.into());
    }
    array.set_index(scope, 1, info_arr.into());

    let by_name = self.collect_modules();
    let by_name_array = v8::Array::new(scope, by_name.len() as i32);
    {
      for (i, (module_type, name, module)) in by_name.into_iter().enumerate() {
        let arr = v8::Array::new(scope, 3);

        let specifier = name.v8(scope);
        arr.set_index(scope, 0, specifier.into());

        let asserted_module_type = v8::Integer::new(scope, module_type as i32);
        arr.set_index(scope, 1, asserted_module_type.into());

        let symbolic_module: v8::Local<v8::Value> = match module {
          SymbolicModule::Alias(alias) => {
            let alias = v8::String::new_from_one_byte(
              scope,
              alias.as_bytes(),
              v8::NewStringType::Normal,
            )
            .unwrap();
            alias.into()
          }
          SymbolicModule::Mod(id) => {
            let id = v8::Integer::new(scope, *id as i32);
            id.into()
          }
        };
        arr.set_index(scope, 2, symbolic_module);

        by_name_array.set_index(scope, i as u32, arr.into());
      }
    }
    array.set_index(scope, 2, by_name_array.into());

    let array_global = v8::Global::new(scope, array);

    let handles = self.handles.clone();
    SnapshottedData {
      module_map_data: array_global,
      module_handles: handles,
    }
  }

  pub fn update_with_snapshotted_data(
    &mut self,
    scope: &mut v8::HandleScope,
    snapshotted_data: SnapshottedData,
  ) {
    let local_data: v8::Local<v8::Array> =
      v8::Local::new(scope, snapshotted_data.module_map_data);

    {
      let next_load_id = local_data.get_index(scope, 0).unwrap();
      assert!(next_load_id.is_int32());
      let integer = next_load_id.to_integer(scope).unwrap();
      let val = integer.int32_value(scope).unwrap();
      self.next_load_id = val;
    }

    {
      let info_val = local_data.get_index(scope, 1).unwrap();

      let info_arr: v8::Local<v8::Array> = info_val.try_into().unwrap();
      let len = info_arr.length() as usize;
      // Over allocate so executing a few scripts doesn't have to resize this vec.
      let mut info = Vec::with_capacity(len + 16);

      for i in 0..len {
        let module_info_arr: v8::Local<v8::Array> = info_arr
          .get_index(scope, i as u32)
          .unwrap()
          .try_into()
          .unwrap();
        let id = module_info_arr
          .get_index(scope, 0)
          .unwrap()
          .to_integer(scope)
          .unwrap()
          .value() as ModuleId;

        let main = module_info_arr
          .get_index(scope, 1)
          .unwrap()
          .to_boolean(scope)
          .is_true();

        let name = module_info_arr
          .get_index(scope, 2)
          .unwrap()
          .to_rust_string_lossy(scope)
          .into();

        let requests_arr: v8::Local<v8::Array> = module_info_arr
          .get_index(scope, 3)
          .unwrap()
          .try_into()
          .unwrap();
        let len = (requests_arr.length() as usize) / 2;
        let mut requests = Vec::with_capacity(len);
        for i in 0..len {
          let specifier = requests_arr
            .get_index(scope, (2 * i) as u32)
            .unwrap()
            .to_rust_string_lossy(scope);
          let asserted_module_type_no = requests_arr
            .get_index(scope, (2 * i + 1) as u32)
            .unwrap()
            .to_integer(scope)
            .unwrap()
            .value();
          let asserted_module_type = match asserted_module_type_no {
            0 => AssertedModuleType::JavaScriptOrWasm,
            1 => AssertedModuleType::Json,
            _ => unreachable!(),
          };
          requests.push(ModuleRequest {
            specifier,
            asserted_module_type,
          });
        }

        let module_type_no = module_info_arr
          .get_index(scope, 4)
          .unwrap()
          .to_integer(scope)
          .unwrap()
          .value();
        let module_type = match module_type_no {
          0 => ModuleType::JavaScript,
          1 => ModuleType::Json,
          _ => unreachable!(),
        };

        let module_info = ModuleInfo {
          id,
          main,
          name,
          requests,
          module_type,
        };
        info.push(module_info);
      }

      self.info = info;
    }

    self
      .by_name_mut(AssertedModuleType::JavaScriptOrWasm)
      .clear();
    self.by_name_mut(AssertedModuleType::Json).clear();

    {
      let by_name_arr: v8::Local<v8::Array> =
        local_data.get_index(scope, 2).unwrap().try_into().unwrap();
      let len = by_name_arr.length() as usize;

      for i in 0..len {
        let arr: v8::Local<v8::Array> = by_name_arr
          .get_index(scope, i as u32)
          .unwrap()
          .try_into()
          .unwrap();

        let specifier =
          arr.get_index(scope, 0).unwrap().to_rust_string_lossy(scope);
        let asserted_module_type = match arr
          .get_index(scope, 1)
          .unwrap()
          .to_integer(scope)
          .unwrap()
          .value()
        {
          0 => AssertedModuleType::JavaScriptOrWasm,
          1 => AssertedModuleType::Json,
          _ => unreachable!(),
        };

        let symbolic_module_val = arr.get_index(scope, 2).unwrap();
        let val = if symbolic_module_val.is_number() {
          SymbolicModule::Mod(
            symbolic_module_val
              .to_integer(scope)
              .unwrap()
              .value()
              .try_into()
              .unwrap(),
          )
        } else {
          SymbolicModule::Alias(
            symbolic_module_val.to_rust_string_lossy(scope).into(),
          )
        };

        self
          .by_name_mut(asserted_module_type)
          .insert(specifier.into(), val);
      }
    }

    self.handles = snapshotted_data.module_handles;
  }

  pub(crate) fn new(
    loader: Rc<ExtModuleLoader>,
    op_state: Rc<RefCell<OpState>>,
  ) -> ModuleMap {
    Self {
      handles: vec![],
      info: vec![],
      by_name_js: HashMap::new(),
      by_name_json: HashMap::new(),
      next_load_id: 1,
      loader,
      op_state,
      dynamic_import_map: HashMap::new(),
      preparing_dynamic_imports: FuturesUnordered::new(),
      pending_dynamic_imports: FuturesUnordered::new(),
      json_value_store: HashMap::new(),
    }
  }

  /// Get module id, following all aliases in case of module specifier
  /// that had been redirected.
  fn get_id(
    &self,
    name: impl AsRef<str>,
    asserted_module_type: AssertedModuleType,
  ) -> Option<ModuleId> {
    let map = self.by_name(asserted_module_type);
    let first_symbolic_module = map.get(name.as_ref())?;
    let mut mod_name = match first_symbolic_module {
      SymbolicModule::Mod(mod_id) => return Some(*mod_id),
      SymbolicModule::Alias(target) => target,
    };
    loop {
      let symbolic_module = map.get(mod_name.as_ref())?;
      match symbolic_module {
        SymbolicModule::Alias(target) => {
          debug_assert!(mod_name != target);
          mod_name = target;
        }
        SymbolicModule::Mod(mod_id) => return Some(*mod_id),
      }
    }
  }

  fn new_json_module(
    &mut self,
    scope: &mut v8::HandleScope,
    name: ModuleName,
    source: ModuleCode,
  ) -> Result<ModuleId, ModuleError> {
    let name_str = name.v8(scope);
    let source_str = v8::String::new_from_utf8(
      scope,
      strip_bom(source.as_bytes()),
      v8::NewStringType::Normal,
    )
    .unwrap();

    let tc_scope = &mut v8::TryCatch::new(scope);

    let parsed_json = match v8::json::parse(tc_scope, source_str) {
      Some(parsed_json) => parsed_json,
      None => {
        assert!(tc_scope.has_caught());
        let exception = tc_scope.exception().unwrap();
        let exception = v8::Global::new(tc_scope, exception);
        return Err(ModuleError::Exception(exception));
      }
    };

    let export_names = [v8::String::new(tc_scope, "default").unwrap()];
    let module = v8::Module::create_synthetic_module(
      tc_scope,
      name_str,
      &export_names,
      json_module_evaluation_steps,
    );

    let handle = v8::Global::<v8::Module>::new(tc_scope, module);
    let value_handle = v8::Global::<v8::Value>::new(tc_scope, parsed_json);
    self.json_value_store.insert(handle.clone(), value_handle);

    let id =
      self.create_module_info(name, ModuleType::Json, handle, false, vec![]);

    Ok(id)
  }

  /// Create and compile an ES module.
  pub(crate) fn new_es_module(
    &mut self,
    scope: &mut v8::HandleScope,
    main: bool,
    name: ModuleName,
    source: ModuleCode,
    is_dynamic_import: bool,
  ) -> Result<ModuleId, ModuleError> {
    let name_str = name.v8(scope);
    let source_str = source.v8(scope);

    let origin = bindings::module_origin(scope, name_str);
    let source = v8::script_compiler::Source::new(source_str, Some(&origin));

    let tc_scope = &mut v8::TryCatch::new(scope);

    let maybe_module = v8::script_compiler::compile_module(tc_scope, source);

    if tc_scope.has_caught() {
      assert!(maybe_module.is_none());
      let exception = tc_scope.exception().unwrap();
      let exception = v8::Global::new(tc_scope, exception);
      return Err(ModuleError::Exception(exception));
    }

    let module = maybe_module.unwrap();

    let mut requests: Vec<ModuleRequest> = vec![];
    let module_requests = module.get_module_requests();
    for i in 0..module_requests.length() {
      let module_request = v8::Local::<v8::ModuleRequest>::try_from(
        module_requests.get(tc_scope, i).unwrap(),
      )
      .unwrap();
      let import_specifier = module_request
        .get_specifier()
        .to_rust_string_lossy(tc_scope);

      let import_assertions = module_request.get_import_assertions();

      let assertions = parse_import_assertions(
        tc_scope,
        import_assertions,
        ImportAssertionsKind::StaticImport,
      );

      // FIXME(bartomieju): there are no stack frames if exception
      // is thrown here
      validate_import_assertions(tc_scope, &assertions);
      if tc_scope.has_caught() {
        let exception = tc_scope.exception().unwrap();
        let exception = v8::Global::new(tc_scope, exception);
        return Err(ModuleError::Exception(exception));
      }

      let module_specifier = match self.loader.resolve(
        &import_specifier,
        name.as_ref(),
        if is_dynamic_import {
          ResolutionKind::DynamicImport
        } else {
          ResolutionKind::Import
        },
      ) {
        Ok(s) => s,
        Err(e) => return Err(ModuleError::Other(e)),
      };
      let asserted_module_type =
        get_asserted_module_type_from_assertions(&assertions);
      let request = ModuleRequest {
        specifier: module_specifier.to_string(),
        asserted_module_type,
      };
      requests.push(request);
    }

    if main {
      let maybe_main_module = self.info.iter().find(|module| module.main);
      if let Some(main_module) = maybe_main_module {
        return Err(ModuleError::Other(generic_error(
          format!("Trying to create \"main\" module ({:?}), when one already exists ({:?})",
          name.as_ref(),
          main_module.name,
        ))));
      }
    }

    let handle = v8::Global::<v8::Module>::new(tc_scope, module);
    let id = self.create_module_info(
      name,
      ModuleType::JavaScript,
      handle,
      main,
      requests,
    );

    Ok(id)
  }

  fn create_module_info(
    &mut self,
    name: FastString,
    module_type: ModuleType,
    handle: v8::Global<v8::Module>,
    main: bool,
    requests: Vec<ModuleRequest>,
  ) -> ModuleId {
    let id = self.handles.len();
    let (name1, name2) = name.into_cheap_copy();
    self
      .by_name_mut(module_type.into())
      .insert(name1, SymbolicModule::Mod(id));
    self.handles.push(handle);
    self.info.push(ModuleInfo {
      id,
      main,
      name: name2,
      requests,
      module_type,
    });

    id
  }

  fn get_requested_modules(&self, id: ModuleId) -> Option<&Vec<ModuleRequest>> {
    self.info.get(id).map(|i| &i.requests)
  }

  fn is_registered(
    &self,
    specifier: impl AsRef<str>,
    asserted_module_type: AssertedModuleType,
  ) -> bool {
    if let Some(id) = self.get_id(specifier.as_ref(), asserted_module_type) {
      let info = self.get_info_by_id(id).unwrap();
      return asserted_module_type == info.module_type.into();
    }

    false
  }

  pub(crate) fn by_name(
    &self,
    asserted_module_type: AssertedModuleType,
  ) -> &HashMap<ModuleName, SymbolicModule> {
    match asserted_module_type {
      AssertedModuleType::Json => &self.by_name_json,
      AssertedModuleType::JavaScriptOrWasm => &self.by_name_js,
    }
  }

  pub(crate) fn by_name_mut(
    &mut self,
    asserted_module_type: AssertedModuleType,
  ) -> &mut HashMap<ModuleName, SymbolicModule> {
    match asserted_module_type {
      AssertedModuleType::Json => &mut self.by_name_json,
      AssertedModuleType::JavaScriptOrWasm => &mut self.by_name_js,
    }
  }

  fn alias(
    &mut self,
    name: FastString,
    asserted_module_type: AssertedModuleType,
    target: FastString,
  ) {
    debug_assert_ne!(name, target);
    self
      .by_name_mut(asserted_module_type)
      .insert(name, SymbolicModule::Alias(target));
  }

  #[cfg(test)]
  fn is_alias(
    &self,
    name: &str,
    asserted_module_type: AssertedModuleType,
  ) -> bool {
    let cond = self.by_name(asserted_module_type).get(name);
    matches!(cond, Some(SymbolicModule::Alias(_)))
  }

  pub(crate) fn get_handle(
    &self,
    id: ModuleId,
  ) -> Option<v8::Global<v8::Module>> {
    self.handles.get(id).cloned()
  }

  pub(crate) fn get_info(
    &self,
    global: &v8::Global<v8::Module>,
  ) -> Option<&ModuleInfo> {
    if let Some(id) = self.handles.iter().position(|module| module == global) {
      return self.info.get(id);
    }

    None
  }

  pub(crate) fn get_info_by_id(&self, id: ModuleId) -> Option<&ModuleInfo> {
    self.info.get(id)
  }

  pub(crate) async fn load_main(
    module_map_rc: Rc<RefCell<ModuleMap>>,
    specifier: impl AsRef<str>,
  ) -> Result<RecursiveModuleLoad, Error> {
    let load =
      RecursiveModuleLoad::main(specifier.as_ref(), module_map_rc.clone());
    load.prepare().await?;
    Ok(load)
  }

  pub(crate) async fn load_side(
    module_map_rc: Rc<RefCell<ModuleMap>>,
    specifier: impl AsRef<str>,
  ) -> Result<RecursiveModuleLoad, Error> {
    let load =
      RecursiveModuleLoad::side(specifier.as_ref(), module_map_rc.clone());
    load.prepare().await?;
    Ok(load)
  }

  // Initiate loading of a module graph imported using `import()`.
  pub(crate) fn load_dynamic_import(
    module_map_rc: Rc<RefCell<ModuleMap>>,
    specifier: &str,
    referrer: &str,
    asserted_module_type: AssertedModuleType,
    resolver_handle: v8::Global<v8::PromiseResolver>,
  ) {
    let load = RecursiveModuleLoad::dynamic_import(
      specifier,
      referrer,
      asserted_module_type,
      module_map_rc.clone(),
    );
    module_map_rc
      .borrow_mut()
      .dynamic_import_map
      .insert(load.id, resolver_handle);

    let loader = module_map_rc.borrow().loader.clone();
    let resolve_result =
      loader.resolve(specifier, referrer, ResolutionKind::DynamicImport);
    let fut = match resolve_result {
      Ok(module_specifier) => {
        if module_map_rc
          .borrow()
          .is_registered(module_specifier, asserted_module_type)
        {
          async move { (load.id, Ok(load)) }.boxed_local()
        } else {
          async move { (load.id, load.prepare().await.map(|()| load)) }
            .boxed_local()
        }
      }
      Err(error) => async move { (load.id, Err(error)) }.boxed_local(),
    };
    module_map_rc
      .borrow_mut()
      .preparing_dynamic_imports
      .push(fut);
  }

  pub(crate) fn has_pending_dynamic_imports(&self) -> bool {
    !(self.preparing_dynamic_imports.is_empty()
      && self.pending_dynamic_imports.is_empty())
  }

  /// Called by `module_resolve_callback` during module instantiation.
  pub(crate) fn resolve_callback<'s>(
    &self,
    scope: &mut v8::HandleScope<'s>,
    specifier: &str,
    referrer: &str,
    import_assertions: HashMap<String, String>,
  ) -> Option<v8::Local<'s, v8::Module>> {
    let resolved_specifier = self
      .loader
      .resolve(specifier, referrer, ResolutionKind::Import)
      .expect("Module should have been already resolved");

    let module_type =
      get_asserted_module_type_from_assertions(&import_assertions);

    if let Some(id) = self.get_id(resolved_specifier.as_str(), module_type) {
      if let Some(handle) = self.get_handle(id) {
        return Some(v8::Local::new(scope, handle));
      }
    }

    None
  }
}
