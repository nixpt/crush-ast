//! `W-VIS02`: `pub fn __x` exports a name marked internal (V1, #519).
//!
//! The `__` prefix is Crush's convention for "internal, even inside the
//! package"; `pub` exports a function to other packages. Both on one function
//! contradict each other, so the compiler warns. The cross-package checks
//! (`W-VIS01`, `N-VIS01`) need the import graph and live in
//! [`crate::import_system`]; this pass needs only the program.

use crate::diagnostics::{CompilerDiagnostic, DiagnosticSeverity};
use crush_cast::Program;
use crush_cast::manifest::SourceLoc;

/// The diagnostic code.
pub const W_VIS02: &str = "W-VIS02";

/// One `W-VIS02` per `pub` function whose own name (after any module
/// prefix) starts with `__`, sorted by file and line.
pub fn check_pub_internal(program: &Program) -> Vec<CompilerDiagnostic> {
    let mut out: Vec<CompilerDiagnostic> = program
        .functions
        .iter()
        .filter(|(_, f)| {
            f.meta
                .get("pub")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false)
        })
        .filter_map(|(mangled, f)| {
            let name = mangled.rsplit("::").next().unwrap_or(mangled);
            if !name.starts_with("__") {
                return None;
            }
            let num = |k: &str| f.meta.get(k).and_then(serde_json::Value::as_u64).unwrap_or(0);
            Some(CompilerDiagnostic {
                code: W_VIS02,
                severity: DiagnosticSeverity::Warning,
                message: format!(
                    "`pub fn {name}` exports a name marked internal: the `__` prefix says \
                     internal, `pub` says exported to other packages"
                ),
                location: SourceLoc {
                    file: f
                        .meta
                        .get("source_file")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                    line: num("line") as u32,
                    col: num("col") as u32,
                },
                hint: Some(format!(
                    "drop `pub` to keep `{name}` inside its package, or rename it without `__` \
                     to export it"
                )),
            })
        })
        .collect();
    out.sort_by(|a, b| {
        (&a.location.file, a.location.line, a.location.col).cmp(&(
            &b.location.file,
            b.location.line,
            b.location.col,
        ))
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pub_dunder_warns_and_plain_does_not() {
        let program = crate::parse_source(
            "pub fn __leak() { return 1 }\npub fn ok() { return 2 }\nfn __quiet() { return 3 }\n",
        )
        .unwrap();
        let d = check_pub_internal(&program);
        assert_eq!(d.len(), 1, "{d:?}");
        assert_eq!(d[0].code, W_VIS02);
        assert!(d[0].message.contains("`pub fn __leak`"), "{}", d[0].message);
        assert_eq!(d[0].location.line, 1);
    }
}
