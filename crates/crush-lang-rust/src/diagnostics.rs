//! Keep the innermost lowering location as errors cross expression/statement
//! boundaries. A tagged error avoids guessing from user-provided source text.
use std::fmt;

#[derive(Debug)]
struct Located(String);

impl fmt::Display for Located {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for Located {}

pub(crate) fn at(kind: &str, span: proc_macro2::Span, error: anyhow::Error) -> anyhow::Error {
    if error.is::<Located>() {
        return error;
    }
    let start = span.start();
    Located(format!(
        "Rust {kind} at {}:{}: {error:#}",
        start.line,
        start.column + 1
    ))
    .into()
}

pub(crate) fn unsupported(span: proc_macro2::Span, construct: &str, reason: &str) -> anyhow::Error {
    let start = span.start();
    Located(format!(
        "unsupported Rust {construct} at {}:{}: {reason}",
        start.line,
        start.column + 1
    ))
    .into()
}

#[cfg(test)]
mod tests {
    #[test]
    fn nested_lowering_keeps_one_precise_location() {
        let error = crate::rust_to_cast("fn main() {\n    let value = 1i64.unsupported();\n}")
            .unwrap_err()
            .to_string();
        assert_eq!(error.matches(" at ").count(), 1, "{error}");
        assert!(error.starts_with("Rust expression at 2:17:"), "{error}");
        let error = crate::rust_to_cast("fn main() {\n    let value = { let n = 1; n };\n}")
            .unwrap_err()
            .to_string();
        assert_eq!(error.matches(" at ").count(), 1, "{error}");
        assert!(error.contains("block expression at 2:17:"), "{error}");
    }
}
