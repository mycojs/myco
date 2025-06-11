#[derive(Debug, thiserror::Error)]
pub enum UtilError {
    #[error("IO error: {source}")]
    Io { #[source] source: std::io::Error },
    
    #[error("File not found: {path}")]
    FileNotFound { path: String },
    
    #[error("Invalid file path: {path}")]
    InvalidFilePath { path: String },
    
    #[error("Failed to read file '{path}': {source}")]
    FileRead { path: String, #[source] source: std::io::Error },
    
    #[error("Failed to write file '{path}': {source}")]
    FileWrite { path: String, #[source] source: std::io::Error },
    
    #[error("Failed to create file '{path}': {source}")]
    FileCreate { path: String, #[source] source: std::io::Error },
    
    #[error("UTF-8 conversion error: {source}")]
    Utf8Conversion { #[source] source: std::string::FromUtf8Error },
    
    // Transpilation errors
    #[error("Transpilation failed: {message}")]
    Transpilation { message: String },
    
    #[error("TypeScript parsing failed: {message}")]
    TypeScriptParsing { message: String },
    
    #[error("Source map generation failed: {message}")]
    SourceMapGeneration { message: String },
    
    #[error("Code generation failed: {message}")]
    CodeGeneration { message: String },
    
    // Zip errors
    #[error("Zip operation failed: {source}")]
    Zip { #[source] source: zip::result::ZipError },
    
    #[error("Source directory not found: {path}")]
    SourceDirectoryNotFound { path: String },
    
    #[error("Failed to strip prefix '{prefix}': {message}")]
    StripPrefix { prefix: String, message: String },
    
    #[error("Failed to create destination file '{path}': {source}")]
    DestinationFileCreate { path: String, #[source] source: std::io::Error },
    
    #[error("Walkdir error: {source}")]
    WalkDir { #[source] source: walkdir::Error },
    
    // URL errors
    #[error("Invalid URL: {message}")]
    InvalidUrl { message: String },
}

impl From<std::io::Error> for UtilError {
    fn from(err: std::io::Error) -> Self {
        UtilError::Io { source: err }
    }
}

impl From<zip::result::ZipError> for UtilError {
    fn from(err: zip::result::ZipError) -> Self {
        UtilError::Zip { source: err }
    }
}

impl From<std::string::FromUtf8Error> for UtilError {
    fn from(err: std::string::FromUtf8Error) -> Self {
        UtilError::Utf8Conversion { source: err }
    }
}

impl From<walkdir::Error> for UtilError {
    fn from(err: walkdir::Error) -> Self {
        UtilError::WalkDir { source: err }
    }
} 