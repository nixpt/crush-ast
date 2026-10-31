use crate::{CrushError, ErrorKind};

impl From<std::io::Error> for CrushError {
    fn from(e: std::io::Error) -> Self {
        let kind = match e.kind() {
            std::io::ErrorKind::NotFound => ErrorKind::NotFound,
            std::io::ErrorKind::PermissionDenied => ErrorKind::PermissionDenied,
            std::io::ErrorKind::InvalidInput => ErrorKind::InvalidArgument,
            std::io::ErrorKind::TimedOut => ErrorKind::Timeout,
            _ => ErrorKind::Io,
        };
        CrushError::new(kind, e.to_string())
    }
}

impl From<std::string::FromUtf8Error> for CrushError {
    fn from(e: std::string::FromUtf8Error) -> Self {
        CrushError::new(ErrorKind::InvalidArgument, e.to_string())
    }
}

impl From<regex::Error> for CrushError {
    fn from(e: regex::Error) -> Self {
        CrushError::new(ErrorKind::InvalidArgument, e.to_string())
    }
}

impl From<std::num::ParseIntError> for CrushError {
    fn from(e: std::num::ParseIntError) -> Self {
        CrushError::new(ErrorKind::InvalidArgument, e.to_string())
    }
}

pub mod casm {
    use crate::{CrushError, ErrorKind};

    #[derive(Debug, Clone)]
    pub enum CasmError {
        UnknownOpcode(String),
        MissingField {
            op: String,
            field: String,
        },
        InvalidHex(String),
        SerializationError(String),
        DeserializationError(String),
        IoError(String),
        /// ECASM integrity check failed
        IntegrityError(String),
        /// ECASM page index out of bounds
        InvalidPageIndex {
            index: u32,
            max: u32,
        },
        /// CASM bytecode format version is incompatible with this runtime.
        /// Carries the unified [`crate::version::VersionMismatch`] shape (VER-05)
        /// with `boundary = casm`, so the failure renders uniformly with the
        /// other version boundaries.
        Version(crate::version::VersionMismatch),
    }

    impl std::fmt::Display for CasmError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            match self {
                Self::UnknownOpcode(op) => write!(f, "unknown opcode: {}", op),
                Self::MissingField { op, field } => {
                    write!(f, "{}: missing {}", op, field)
                }
                Self::InvalidHex(msg) => write!(f, "invalid hex: {}", msg),
                Self::SerializationError(msg) => write!(f, "serialization error: {}", msg),
                Self::DeserializationError(msg) => write!(f, "deserialization error: {}", msg),
                Self::IoError(msg) => write!(f, "I/O error: {}", msg),
                Self::IntegrityError(msg) => write!(f, "integrity error: {}", msg),
                Self::InvalidPageIndex { index, max } => {
                    write!(f, "page index {} out of bounds (max {})", index, max)
                }
                Self::Version(v) => write!(f, "{}", v),
            }
        }
    }

    impl std::error::Error for CasmError {}

    impl From<CasmError> for CrushError {
        fn from(e: CasmError) -> Self {
            match e {
                CasmError::UnknownOpcode(op) => CrushError::new(
                    ErrorKind::InvalidArgument,
                    format!("unknown opcode: {}", op),
                ),
                CasmError::MissingField { op, field } => CrushError::new(
                    ErrorKind::InvalidArgument,
                    format!("{}: missing {}", op, field),
                ),
                CasmError::InvalidHex(msg) => {
                    CrushError::new(ErrorKind::InvalidArgument, format!("invalid hex: {}", msg))
                }
                CasmError::SerializationError(msg) => {
                    CrushError::new(ErrorKind::Internal, format!("serialization error: {}", msg))
                }
                CasmError::DeserializationError(msg) => CrushError::new(
                    ErrorKind::InvalidArgument,
                    format!("deserialization error: {}", msg),
                ),
                CasmError::IoError(msg) => CrushError::new(ErrorKind::Io, msg),
                CasmError::IntegrityError(msg) => {
                    CrushError::new(ErrorKind::Internal, format!("integrity error: {}", msg))
                }
                CasmError::InvalidPageIndex { index, max } => CrushError::new(
                    ErrorKind::InvalidArgument,
                    format!("page index {} out of bounds (max {})", index, max),
                ),
                CasmError::Version(v) => v.into(),
            }
        }
    }
}
