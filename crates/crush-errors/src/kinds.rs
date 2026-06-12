use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ErrorKind {
    #[error("permission denied")]
    PermissionDenied,

    #[error("not found")]
    NotFound,

    #[error("invalid argument")]
    InvalidArgument,

    #[error("type mismatch")]
    TypeMismatch,

    #[error("capability violation")]
    CapabilityViolation,

    #[error("resource exhausted")]
    ResourceExhausted,

    #[error("unsupported operation")]
    Unsupported,

    #[error("I/O error")]
    Io,

    #[error("internal error")]
    Internal,

    #[error("cancelled")]
    Cancelled,

    #[error("timeout")]
    Timeout,

    #[error("already exists")]
    AlreadyExists,
}
