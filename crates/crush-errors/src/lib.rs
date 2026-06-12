mod context;
pub mod convert;
mod kinds;
pub mod version;

pub use context::{CrushError, ErrorContext, ResultExt};
pub use kinds::ErrorKind;
pub use version::{VersionBoundary, VersionMismatch};

pub type CrushResult<T> = Result<T, CrushError>;
