use thiserror::Error;

#[derive(Debug, Error)]
pub enum CodegenError {
    #[error("Parse error: {0}")]
    Parse(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("No machines found in input")]
    NoMachines,

    #[error("Unsupported type: {0}")]
    UnsupportedType(String),
}

pub type CodegenResult<T> = Result<T, CodegenError>;
