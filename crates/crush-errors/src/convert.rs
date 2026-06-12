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

pub mod vm {
    use crate::{CrushError, ErrorKind};

    #[derive(Debug, Clone)]
    pub enum RuntimeError {
        StackUnderflow,
        VariableNotFound(String),
        FunctionNotFound(String),
        CapabilityNotFound(String),
        TypeMismatch(String),
        PermissionDenied(String),
        ArenaError(String),
        InternalError(String),
        GasExceeded,
        VmCancelled,
        WatchdogTriggered(String),
    }

    impl From<RuntimeError> for CrushError {
        fn from(e: RuntimeError) -> Self {
            match e {
                RuntimeError::StackUnderflow => {
                    CrushError::new(ErrorKind::Internal, "stack underflow")
                }
                RuntimeError::VariableNotFound(name) => CrushError::new(
                    ErrorKind::NotFound,
                    format!("variable '{}' not found", name),
                ),
                RuntimeError::FunctionNotFound(name) => CrushError::new(
                    ErrorKind::NotFound,
                    format!("function '{}' not found", name),
                ),
                RuntimeError::CapabilityNotFound(name) => CrushError::new(
                    ErrorKind::CapabilityViolation,
                    format!("capability '{}' not found", name),
                ),
                RuntimeError::TypeMismatch(msg) => CrushError::new(ErrorKind::TypeMismatch, msg),
                RuntimeError::PermissionDenied(msg) => {
                    CrushError::new(ErrorKind::PermissionDenied, msg)
                }
                RuntimeError::ArenaError(msg) => {
                    CrushError::new(ErrorKind::Internal, format!("arena error: {}", msg))
                }
                RuntimeError::InternalError(msg) => CrushError::new(ErrorKind::Internal, msg),
                RuntimeError::GasExceeded => {
                    CrushError::new(ErrorKind::ResourceExhausted, "gas limit exceeded")
                }
                RuntimeError::VmCancelled => CrushError::new(ErrorKind::Cancelled, "VM cancelled"),
                RuntimeError::WatchdogTriggered(action) => CrushError::new(
                    ErrorKind::Timeout,
                    format!("watchdog triggered: {}", action),
                ),
            }
        }
    }

    #[derive(Debug, Clone)]
    pub enum VmError {
        InvalidState,
        RuntimeError(RuntimeError),
    }

    impl From<VmError> for CrushError {
        fn from(e: VmError) -> Self {
            match e {
                VmError::InvalidState => {
                    CrushError::new(ErrorKind::Internal, "invalid VM state transition")
                }
                VmError::RuntimeError(re) => re.into(),
            }
        }
    }

    #[derive(Debug, Clone)]
    pub enum SchedulerError {
        VmNotFound(u64),
        RuntimeError(RuntimeError),
    }

    impl From<SchedulerError> for CrushError {
        fn from(e: SchedulerError) -> Self {
            match e {
                SchedulerError::VmNotFound(id) => {
                    CrushError::new(ErrorKind::NotFound, format!("VM {} not found", id))
                }
                SchedulerError::RuntimeError(re) => re.into(),
            }
        }
    }

    #[derive(Debug)]
    pub enum CbvError {
        InvalidMagic,
        UnsupportedVersion(u16),
        InvalidTypeTag(u8),
        InvalidUtf8,
        UnexpectedEof,
        IoError(String),
        InvalidReference(usize),
    }

    impl From<CbvError> for CrushError {
        fn from(e: CbvError) -> Self {
            match e {
                CbvError::InvalidMagic => {
                    CrushError::new(ErrorKind::InvalidArgument, "invalid CBV magic bytes")
                }
                CbvError::UnsupportedVersion(v) => CrushError::new(
                    ErrorKind::Unsupported,
                    format!("unsupported CBV version: {}", v),
                ),
                CbvError::InvalidTypeTag(t) => CrushError::new(
                    ErrorKind::InvalidArgument,
                    format!("invalid type tag: 0x{:02x}", t),
                ),
                CbvError::InvalidUtf8 => {
                    CrushError::new(ErrorKind::InvalidArgument, "invalid UTF-8 in CBV data")
                }
                CbvError::UnexpectedEof => {
                    CrushError::new(ErrorKind::InvalidArgument, "unexpected end of CBV data")
                }
                CbvError::IoError(msg) => CrushError::new(ErrorKind::Io, msg),
                CbvError::InvalidReference(idx) => CrushError::new(
                    ErrorKind::InvalidArgument,
                    format!("invalid arena reference: {}", idx),
                ),
            }
        }
    }

    #[derive(Debug, Clone, PartialEq)]
    pub enum BinaryError {
        BoundsError,
        TypeError,
    }

    impl From<BinaryError> for CrushError {
        fn from(e: BinaryError) -> Self {
            match e {
                BinaryError::BoundsError => {
                    CrushError::new(ErrorKind::InvalidArgument, "binary access out of bounds")
                }
                BinaryError::TypeError => {
                    CrushError::new(ErrorKind::TypeMismatch, "expected bytes or buffer type")
                }
            }
        }
    }
}

pub mod hal {
    use crate::{CrushError, ErrorKind};

    #[derive(Debug, Clone)]
    pub enum HalError {
        Unsupported,
        PermissionDenied(String),
        NotFound,
        InvalidArgument,
        Io(String),
        OutOfMemory,
        Other(String),
    }

    impl From<HalError> for CrushError {
        fn from(e: HalError) -> Self {
            match e {
                HalError::Unsupported => {
                    CrushError::new(ErrorKind::Unsupported, "unsupported platform operation")
                }
                HalError::PermissionDenied(msg) => {
                    CrushError::new(ErrorKind::PermissionDenied, msg)
                }
                HalError::NotFound => CrushError::new(ErrorKind::NotFound, "resource not found"),
                HalError::InvalidArgument => {
                    CrushError::new(ErrorKind::InvalidArgument, "invalid argument")
                }
                HalError::Io(msg) => CrushError::new(ErrorKind::Io, msg),
                HalError::OutOfMemory => {
                    CrushError::new(ErrorKind::ResourceExhausted, "out of memory")
                }
                HalError::Other(msg) => CrushError::new(ErrorKind::Internal, msg),
            }
        }
    }

    #[derive(Debug, Clone)]
    pub enum HostDispatchError {
        NotFound(String),
        UnknownOperation(String),
        PermissionDenied(String),
        InvalidArguments(String),
        Internal(String),
        Await(u64),
    }

    impl From<HostDispatchError> for CrushError {
        fn from(e: HostDispatchError) -> Self {
            match e {
                HostDispatchError::NotFound(s) => CrushError::new(
                    ErrorKind::NotFound,
                    format!("host capsule not found: {}", s),
                ),
                HostDispatchError::UnknownOperation(op) => {
                    CrushError::new(ErrorKind::NotFound, format!("unknown operation: {}", op))
                }
                HostDispatchError::PermissionDenied(msg) => {
                    CrushError::new(ErrorKind::PermissionDenied, msg)
                }
                HostDispatchError::InvalidArguments(msg) => {
                    CrushError::new(ErrorKind::InvalidArgument, msg)
                }
                HostDispatchError::Internal(msg) => CrushError::new(ErrorKind::Internal, msg),
                HostDispatchError::Await(_id) => CrushError::new(
                    ErrorKind::Internal,
                    "async await not supported in this context",
                ),
            }
        }
    }
}

pub mod exo {
    use crate::{CrushError, ErrorKind};

    #[derive(Debug, Clone)]
    pub enum HostError {
        UnknownOperation(String),
        PermissionDenied(String),
        InvalidArguments(String),
        NotFound(String),
        Internal(String),
        Await(u64),
    }

    impl From<HostError> for CrushError {
        fn from(e: HostError) -> Self {
            match e {
                HostError::UnknownOperation(op) => {
                    CrushError::new(ErrorKind::NotFound, format!("unknown operation: {}", op))
                }
                HostError::PermissionDenied(msg) => {
                    CrushError::new(ErrorKind::PermissionDenied, msg)
                }
                HostError::InvalidArguments(msg) => {
                    CrushError::new(ErrorKind::InvalidArgument, msg)
                }
                HostError::NotFound(what) => CrushError::new(ErrorKind::NotFound, what),
                HostError::Internal(msg) => CrushError::new(ErrorKind::Internal, msg),
                HostError::Await(_id) => CrushError::new(
                    ErrorKind::Internal,
                    "async await not supported in this context",
                ),
            }
        }
    }

    #[derive(Debug, Clone)]
    pub enum LoaderError {
        NotFound(String),
        InvalidManifest(String),
        DependencyError(String),
        CircularDependency(String),
        VersionMismatch(String),
        PermissionDenied(String),
        ResourceExceeded(String),
        IoError(String),
        ParseError(String),
    }

    impl From<LoaderError> for CrushError {
        fn from(e: LoaderError) -> Self {
            match e {
                LoaderError::NotFound(s) => {
                    CrushError::new(ErrorKind::NotFound, format!("capsule not found: {}", s))
                }
                LoaderError::InvalidManifest(s) => CrushError::new(
                    ErrorKind::InvalidArgument,
                    format!("invalid manifest: {}", s),
                ),
                LoaderError::DependencyError(s) => CrushError::new(
                    ErrorKind::InvalidArgument,
                    format!("dependency resolution failed: {}", s),
                ),
                LoaderError::CircularDependency(s) => CrushError::new(
                    ErrorKind::InvalidArgument,
                    format!("circular dependency: {}", s),
                ),
                LoaderError::VersionMismatch(s) => CrushError::new(
                    ErrorKind::InvalidArgument,
                    format!("version mismatch: {}", s),
                ),
                LoaderError::PermissionDenied(s) => CrushError::new(ErrorKind::PermissionDenied, s),
                LoaderError::ResourceExceeded(s) => {
                    CrushError::new(ErrorKind::ResourceExhausted, s)
                }
                LoaderError::IoError(s) => CrushError::new(ErrorKind::Io, s),
                LoaderError::ParseError(s) => {
                    CrushError::new(ErrorKind::InvalidArgument, format!("parse error: {}", s))
                }
            }
        }
    }

    #[derive(Debug, Clone)]
    pub enum IpcError {
        QueueNotFound(String),
        PermissionDenied(String),
        QueueFull,
        QueueEmpty,
        InvalidHandle(String),
        CryptoError(String),
    }

    impl From<IpcError> for CrushError {
        fn from(e: IpcError) -> Self {
            match e {
                IpcError::QueueNotFound(id) => {
                    CrushError::new(ErrorKind::NotFound, format!("queue not found: {}", id))
                }
                IpcError::PermissionDenied(msg) => {
                    CrushError::new(ErrorKind::PermissionDenied, msg)
                }
                IpcError::QueueFull => CrushError::new(ErrorKind::ResourceExhausted, "queue full"),
                IpcError::QueueEmpty => CrushError::new(ErrorKind::NotFound, "queue empty"),
                IpcError::InvalidHandle(h) => {
                    CrushError::new(ErrorKind::InvalidArgument, format!("invalid handle: {}", h))
                }
                IpcError::CryptoError(msg) => {
                    CrushError::new(ErrorKind::Internal, format!("crypto error: {}", msg))
                }
            }
        }
    }

    #[derive(Debug, Clone)]
    pub enum CryptoError {
        EncryptionFailed(String),
        DecryptionFailed(String),
        KeyNotAvailable,
        InvalidData(String),
    }

    impl From<CryptoError> for CrushError {
        fn from(e: CryptoError) -> Self {
            match e {
                CryptoError::EncryptionFailed(msg) => {
                    CrushError::new(ErrorKind::Internal, format!("encryption failed: {}", msg))
                }
                CryptoError::DecryptionFailed(msg) => {
                    CrushError::new(ErrorKind::Internal, format!("decryption failed: {}", msg))
                }
                CryptoError::KeyNotAvailable => {
                    CrushError::new(ErrorKind::NotFound, "encryption key not available")
                }
                CryptoError::InvalidData(msg) => {
                    CrushError::new(ErrorKind::InvalidArgument, format!("invalid data: {}", msg))
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_io_error_conversion() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file missing");
        let crush_err: CrushError = io_err.into();
        assert_eq!(crush_err.kind(), &ErrorKind::NotFound);
    }

    #[test]
    fn test_permission_denied_conversion() {
        let io_err = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "access denied");
        let crush_err: CrushError = io_err.into();
        assert_eq!(crush_err.kind(), &ErrorKind::PermissionDenied);
    }

    #[test]
    fn test_runtime_error_conversion() {
        let rt_err = vm::RuntimeError::CapabilityNotFound("fs.read".into());
        let crush_err: CrushError = rt_err.into();
        assert_eq!(crush_err.kind(), &ErrorKind::CapabilityViolation);
        assert!(crush_err.message().contains("fs.read"));
    }

    #[test]
    fn test_hal_error_conversion() {
        let hal_err = hal::HalError::OutOfMemory;
        let crush_err: CrushError = hal_err.into();
        assert_eq!(crush_err.kind(), &ErrorKind::ResourceExhausted);
    }

    #[test]
    fn test_loader_error_conversion() {
        let loader_err = exo::LoaderError::CircularDependency("a -> b -> a".into());
        let crush_err: CrushError = loader_err.into();
        assert_eq!(crush_err.kind(), &ErrorKind::InvalidArgument);
        assert!(crush_err.message().contains("circular"));
    }

    #[test]
    fn test_ipc_error_conversion() {
        let ipc_err = exo::IpcError::QueueFull;
        let crush_err: CrushError = ipc_err.into();
        assert_eq!(crush_err.kind(), &ErrorKind::ResourceExhausted);
    }

    #[test]
    fn test_casm_error_conversion() {
        let casm_err = casm::CasmError::UnknownOpcode("foo".into());
        let crush_err: CrushError = casm_err.into();
        assert_eq!(crush_err.kind(), &ErrorKind::InvalidArgument);
        assert!(crush_err.message().contains("foo"));
    }
}

pub mod stdlib {
    use crate::{CrushError, ErrorKind};

    #[derive(Debug, Clone)]
    pub enum StdlibError {
        InvalidArgCount { expected: usize, got: usize },
        InvalidArgument(String),
        IoError(String),
        FsError(String),
        TypeError(String),
        NotFound(String),
    }

    impl std::fmt::Display for StdlibError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            match self {
                Self::InvalidArgCount { expected, got } => {
                    write!(f, "expected {} argument(s), got {}", expected, got)
                }
                Self::InvalidArgument(msg) => write!(f, "invalid argument: {}", msg),
                Self::IoError(msg) => write!(f, "I/O error: {}", msg),
                Self::FsError(msg) => write!(f, "filesystem error: {}", msg),
                Self::TypeError(msg) => write!(f, "type error: {}", msg),
                Self::NotFound(what) => write!(f, "not found: {}", what),
            }
        }
    }

    impl std::error::Error for StdlibError {}

    impl From<StdlibError> for CrushError {
        fn from(e: StdlibError) -> Self {
            match e {
                StdlibError::InvalidArgCount { expected, got } => CrushError::new(
                    ErrorKind::InvalidArgument,
                    format!("expected {} argument(s), got {}", expected, got),
                ),
                StdlibError::InvalidArgument(msg) => {
                    CrushError::new(ErrorKind::InvalidArgument, msg)
                }
                StdlibError::IoError(msg) => CrushError::new(ErrorKind::Io, msg),
                StdlibError::FsError(msg) => {
                    CrushError::new(ErrorKind::Io, format!("filesystem: {}", msg))
                }
                StdlibError::TypeError(msg) => CrushError::new(ErrorKind::TypeMismatch, msg),
                StdlibError::NotFound(what) => CrushError::new(ErrorKind::NotFound, what),
            }
        }
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
