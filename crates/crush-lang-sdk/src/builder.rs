//! Programmatic builder for CVM1 programs.
//!
//! [`ProgramBuilder`] lets you assemble a CVM1 program from CASM text lines
//! and a permission list without manually calling [`crush_vm::assemble`].

use crush_vm::{Program, assemble};

/// Errors produced while building a program.
#[derive(Debug, thiserror::Error)]
#[error("program build failed: {0}")]
pub struct ProgramBuilderError(pub String);

/// Builder for a CVM1 [`Program`].
///
/// # Example
///
/// ```rust
/// use crush_lang_sdk::ProgramBuilder;
///
/// let program = ProgramBuilder::new()
///     .name("hello")
///     .permission("io.print")
///     .line(".func main")
///     .line(r#"PUSH_STR "hello""#)
///     .line(r#"CAP_CALL "io.print" 1"#)
///     .line("HALT")
///     .build()
///     .expect("valid casm");
///
/// assert_eq!(program.manifest.name.as_deref(), Some("hello"));
/// ```
#[derive(Debug, Default, Clone)]
pub struct ProgramBuilder {
    source: String,
    permissions: Vec<String>,
    name: Option<String>,
}

impl ProgramBuilder {
    /// Create a new, empty builder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the program name stored in the manifest.
    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// Grant a capability permission. May be called multiple times.
    pub fn permission(mut self, cap: impl Into<String>) -> Self {
        self.permissions.push(cap.into());
        self
    }

    /// Add a line of CASM source. Leading/trailing whitespace is trimmed.
    pub fn line(mut self, line: impl AsRef<str>) -> Self {
        let trimmed = line.as_ref().trim();
        if !trimmed.is_empty() {
            self.source.push_str(trimmed);
            self.source.push('\n');
        }
        self
    }

    /// Add multiple lines of CASM source at once.
    pub fn lines(mut self, lines: impl IntoIterator<Item = impl AsRef<str>>) -> Self {
        for line in lines {
            self = self.line(line);
        }
        self
    }

    /// Assemble the accumulated source into a [`Program`].
    pub fn build(self) -> Result<Program, ProgramBuilderError> {
        let permissions: Vec<&str> = self.permissions.iter().map(|s| s.as_str()).collect();
        assemble(&self.source, Some(&permissions), self.name.as_deref())
            .map_err(|e| ProgramBuilderError(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builder_produces_program() {
        let program = ProgramBuilder::new()
            .name("test")
            .permission("io.print")
            .line(".func main")
            .line(r#"PUSH_STR "x""#)
            .line(r#"CAP_CALL "io.print" 1"#)
            .line("HALT")
            .build()
            .expect("build should succeed");

        assert_eq!(program.manifest.name.as_deref(), Some("test"));
        assert!(program.manifest.permissions.contains(&"io.print".to_string()));
        assert!(!program.code.is_empty());
    }

    #[test]
    fn empty_lines_are_ignored() {
        let program = ProgramBuilder::new()
            .permission("io.print")
            .line("")
            .line(".func main")
            .line("   ")
            .line("HALT")
            .build()
            .expect("build should succeed");

        assert_eq!(program.manifest.functions.len(), 1);
    }

    #[test]
    fn invalid_casm_fails() {
        let err = ProgramBuilder::new()
            .line("UNKNOWN_OPCODE")
            .build()
            .expect_err("build should fail");

        assert!(err.to_string().contains("unknown opcode"));
    }
}
