use crate::deps::LockFileDiff;

#[derive(Debug, thiserror::Error)]
pub enum MycoError {
    #[error("File not found: {path}")]
    FileNotFound { path: String },
    
    #[error("Path is not a file: {path}")]
    NotAFile { path: String },
    
    #[error("Failed to canonicalize path '{path}': {source}")]
    PathCanonicalization { path: String, #[source] source: std::io::Error },
    
    #[error("Failed to get current directory: {source}")]
    CurrentDirectory { #[source] source: std::io::Error },
    
    #[error("Failed to create Tokio runtime: {source}")]
    TokioRuntime { #[source] source: tokio::io::Error },
    
    #[error("Failed to read script file '{path}': {source}")]
    ReadFile { path: String, #[source] source: std::io::Error },
    
    #[error("Failed to compile script: {message}")]
    ScriptCompilation { message: String },
    
    #[error("Failed to run script: {message}")]
    ScriptExecution { message: String },
    
    #[error("Failed to compile main module: {message}")]
    MainModuleCompilation { message: String },
    
    #[error("Failed to instantiate main module: {message}")]
    MainModuleInstantiation { message: String },
    
    #[error("Module evaluation failed: {message}")]
    ModuleEvaluation { message: String },
    
    #[error("Failed to compile promise handler")]
    PromiseHandler,
    
    #[error("Failed to run promise handler")]
    PromiseHandlerExecution,
    
    #[error("Failed to compile runtime script")]
    RuntimeCompilation,
    
    #[error("Failed to run runtime script")]
    RuntimeExecution,
    
    #[error("V8 string creation failed")]
    V8StringCreation,
    
    #[error("ICU data initialization failed")]
    IcuDataInit,
    
    #[error("V8 context creation failed")]
    V8ContextCreation,
    
    #[error("Module not found: {specifier} (resolved to: {resolved_path})")]
    ModuleNotFound { specifier: String, resolved_path: String },
    
    #[error("Failed to transpile {path}: {message}")]
    Transpilation { path: String, message: String },
    
    #[error("Invalid UTF-8 in source map: {source}")]
    InvalidSourceMapUtf8 { #[source] source: std::string::FromUtf8Error },
    
    #[error("Failed to compile module: {specifier} (resolved to: {resolved_path})")]
    ModuleCompilation { specifier: String, resolved_path: String },
    
    #[error("Event loop error: {message}")]
    EventLoop { message: String },
    
    #[error("Unhandled error: {message}")]
    UnhandledError { message: String },
    
    #[error("Inspector error: {message}")]
    Inspector { message: String },
    
    #[error("Operation failed: {message}")]
    Operation { message: String },
    
    #[error("Internal error: {message}")]
    Internal { message: String },
    
    #[error("Failed to resolve dependencies: {message}")]
    DependencyResolution { message: String },
    
    #[error("Lockfile mismatch - run `myco install --save` to update\n{diff}")]
    LockfileMismatch { diff: LockFileDiff },
    
    #[error("Failed to load lockfile - have you run `myco install --save`?")]
    LockfileLoad,
    
    #[error("Package integrity check failed for {package}: expected {expected}, got {actual}")]
    IntegrityMismatch { package: String, expected: String, actual: String },
    
    #[error("Failed to download package from {url}: {source}")]
    PackageDownload { url: String, #[source] source: Box<dyn std::error::Error + Send + Sync> },
    
    #[error("Failed to extract package: {source}")]
    PackageExtraction { #[source] source: zip::result::ZipError },
    
    #[error("Package {package} not found in any registries")]
    PackageNotFound { package: String },
    
    #[error("No registries found in myco.toml")]
    NoRegistries,
    
    #[error("Failed to save lockfile: {source}")]
    LockfileSave { #[source] source: std::io::Error },
    
    #[error("Failed to create vendor directory: {source}")]
    VendorDirCreation { #[source] source: std::io::Error },
    
    #[error("Failed to write file '{path}': {source}")]
    FileWrite { path: String, #[source] source: std::io::Error },
    
    #[error("Failed to parse myco.toml: {source}")]
    ManifestParse { #[source] source: toml::de::Error },
    
    #[error("No myco.toml found starting from directory '{start_dir}'")]
    ManifestNotFound { start_dir: String },
    
    #[error("Failed to serialize myco.toml: {source}")]
    ManifestSerialize { #[source] source: toml::ser::Error },
    
    #[error("Failed to serialize JSON: {source}")]
    JsonSerialize { #[source] source: serde_json::Error },
    
    #[error("Invalid package name: {name}")]
    InvalidPackageName { name: String },
    
    #[error("Invalid package version: {version}")]
    InvalidPackageVersion { version: String },
    
    #[error("Invalid version string: {0}")]
    InvalidVersionString(String),
    
    #[error("Invalid URL: {url}")]
    InvalidUrl { url: String },
    
    #[error("Directory already exists: {path}")]
    DirectoryExists { path: String },
    
    #[error("Failed to create directory '{path}': {source}")]
    DirectoryCreation { path: String, #[source] source: std::io::Error },
    
    #[error("Failed to extract init files: {source}")]
    InitFileExtraction { #[source] source: zip::result::ZipError },
    
    #[error("Failed to load generated myco.toml: {source}")]
    InitManifestLoad { #[source] source: Box<MycoError> },
    
    #[error("No package definition found in myco.toml")]
    NoPackageDefinition,
    
    #[error("Registry '{name}' not found in myco.toml")]
    RegistryNotFound { name: String },
    
    #[error("Publishing to URL registries is not yet supported")]
    UrlRegistryNotSupported,
    
    #[error("Invalid registry format: {message}")]
    InvalidRegistryFormat { message: String },
    
    #[error("Version {version} already exists")]
    VersionExists { version: String },
    
    #[error("dist directory not found")]
    DistDirectoryNotFound,
    
    #[error("Failed to parse registry TOML: {source}")]
    RegistryParse { #[source] source: toml_edit::TomlError },
    
    #[error("Failed to determine package version")]
    PackageVersionDetermination,
    
    #[error("Failed to create package archive: {source}")]
    ArchiveCreation { #[source] source: std::io::Error },
}

// Only implement From for std::io::Error, not tokio::io::Error to avoid conflicts
impl From<std::io::Error> for MycoError {
    fn from(err: std::io::Error) -> Self {
        MycoError::Internal { message: err.to_string() }
    }
}

impl From<util::UtilError> for MycoError {
    fn from(err: util::UtilError) -> Self {
        match err {
            util::UtilError::FileNotFound { path } => {
                MycoError::FileNotFound { path }
            }
            util::UtilError::FileRead { path, source } => {
                MycoError::ReadFile { path, source }
            }
            util::UtilError::FileWrite { path, source } => {
                MycoError::FileWrite { path, source }
            }
            util::UtilError::Transpilation { message } => {
                MycoError::Transpilation { path: "unknown".to_string(), message }
            }
            util::UtilError::TypeScriptParsing { message } => {
                MycoError::Transpilation { path: "unknown".to_string(), message }
            }
            util::UtilError::CodeGeneration { message } => {
                MycoError::Transpilation { path: "unknown".to_string(), message }
            }
            util::UtilError::SourceMapGeneration { message } => {
                MycoError::Transpilation { path: "unknown".to_string(), message }
            }
            util::UtilError::InvalidFilePath { path } => {
                MycoError::Transpilation { path, message: "Invalid file path".to_string() }
            }
            _ => {
                MycoError::Internal { message: err.to_string() }
            }
        }
    }
}