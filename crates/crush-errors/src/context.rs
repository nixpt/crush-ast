use crate::ErrorKind;
use std::fmt;

#[derive(Debug)]
pub struct CrushError {
    kind: ErrorKind,
    message: String,
    source: Option<Box<dyn std::error::Error + Send + Sync + 'static>>,
}

impl CrushError {
    pub fn new(kind: ErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            source: None,
        }
    }

    pub fn with_source<E>(mut self, source: E) -> Self
    where
        E: std::error::Error + Send + Sync + 'static,
    {
        self.source = Some(Box::new(source));
        self
    }

    pub fn kind(&self) -> &ErrorKind {
        &self.kind
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub fn permission_denied(msg: impl Into<String>) -> Self {
        Self::new(ErrorKind::PermissionDenied, msg)
    }

    pub fn not_found(msg: impl Into<String>) -> Self {
        Self::new(ErrorKind::NotFound, msg)
    }

    pub fn invalid_argument(msg: impl Into<String>) -> Self {
        Self::new(ErrorKind::InvalidArgument, msg)
    }

    pub fn type_mismatch(msg: impl Into<String>) -> Self {
        Self::new(ErrorKind::TypeMismatch, msg)
    }

    pub fn capability_violation(msg: impl Into<String>) -> Self {
        Self::new(ErrorKind::CapabilityViolation, msg)
    }

    pub fn internal(msg: impl Into<String>) -> Self {
        Self::new(ErrorKind::Internal, msg)
    }

    pub fn io(msg: impl Into<String>) -> Self {
        Self::new(ErrorKind::Io, msg)
    }

    pub fn cancelled(msg: impl Into<String>) -> Self {
        Self::new(ErrorKind::Cancelled, msg)
    }
}

impl fmt::Display for CrushError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.kind, self.message)
    }
}

impl std::error::Error for CrushError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.source
            .as_ref()
            .map(|e| e.as_ref() as &(dyn std::error::Error + 'static))
    }
}

pub trait ErrorContext<T> {
    fn context(self, msg: impl Into<String>) -> Result<T, CrushError>;
    fn with_context<F, S>(self, f: F) -> Result<T, CrushError>
    where
        F: FnOnce() -> S,
        S: Into<String>;
}

pub trait ResultExt<T, E> {
    fn into_crush(self) -> Result<T, CrushError>;
}

impl<T, E> ErrorContext<T> for Result<T, E>
where
    E: std::error::Error + Send + Sync + 'static,
{
    fn context(self, msg: impl Into<String>) -> Result<T, CrushError> {
        self.map_err(|e| CrushError::new(ErrorKind::Internal, msg).with_source(e))
    }

    fn with_context<F, S>(self, f: F) -> Result<T, CrushError>
    where
        F: FnOnce() -> S,
        S: Into<String>,
    {
        self.map_err(|e| CrushError::new(ErrorKind::Internal, f()).with_source(e))
    }
}
