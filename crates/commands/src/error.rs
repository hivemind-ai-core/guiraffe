//! Errors are data; invalid registration never partially mutates the catalog.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    #[error("invalid command slug: {0}")]
    InvalidId(String),
    #[error("invalid keybinding: {0}")]
    InvalidBinding(String),
    #[error("invalid context expression: {0}")]
    InvalidContext(String),
    #[error("duplicate command: {0}")]
    Duplicate(String),
    #[error("unknown command: {0}")]
    NotFound(String),
    #[error("command title must not be empty")]
    EmptyTitle,
    #[error("command is disabled: {0}")]
    Disabled(String),
    #[error("command context is not active: {0}")]
    Inactive(String),
    #[error("native keybinding rejected: {0}")]
    NativeBinding(String),
    #[error("invalid command parameter: {0}")]
    InvalidParameter(String),
    #[error("missing or invalid arguments: {0}")]
    InvalidArguments(String),
    #[error("commands service is not initialized")]
    NotInitialized,
}
