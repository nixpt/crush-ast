//! Execution profiles for embedded RustPython.
//!
//! Profiles control which builtins, modules, and capabilities are
//! available to Python code running inside CrushVM.

/// Execution profile selection.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PythonProfile {
    /// Tiny Python: expressions, functions, classes, basic builtins.
    /// No host imports, no I/O. Pure computation only.
    Core,
    /// Core + exo.* capability modules (fs, log, env, clock, etc.).
    Exo,
    /// RustPython's broader stdlib, still capability-gated.
    Compat,
    /// Full CPython via subprocess (external).
    External,
}

impl PythonProfile {
    pub fn allows_import(&self, module: &str) -> bool {
        match self {
            PythonProfile::Core => false,
            PythonProfile::Exo => module.starts_with("exo.") || module == "exo",
            PythonProfile::Compat => {
                let allowed = [
                    "exo", "exo.fs", "exo.log", "exo.env", "exo.clock",
                    "exo.json", "exo.random", "exo.http", "exo.cap",
                    "math", "json", "re", "string", "collections",
                    "datetime", "enum", "functools", "itertools",
                    "copy", "types", "textwrap", "statistics",
                ];
                allowed.contains(&module)
            }
            PythonProfile::External => true,
        }
    }

    pub fn allows_builtin(&self, name: &str) -> BuiltinDecision {
        match self {
            PythonProfile::Core | PythonProfile::Exo | PythonProfile::Compat => {
                match name {
                    "print" | "len" | "range" | "int" | "float" | "str"
                    | "bool" | "list" | "dict" | "tuple" | "set"
                    | "type" | "isinstance" | "issubclass" | "hasattr"
                    | "getattr" | "setattr" | "delattr" | "repr"
                    | "abs" | "min" | "max" | "sum" | "round"
                    | "sorted" | "reversed" | "enumerate" | "zip"
                    | "map" | "filter" | "any" | "all" | "callable"
                    | "next" | "iter" | "id" | "hash" | "hex" | "oct"
                    | "bin" | "ord" | "chr" | "format" | "ascii"
                    | "staticmethod" | "classmethod" | "property"
                    | "super" | "object" | "memoryview" | "bytes"
                    | "bytearray" | "slice" | "divmod" | "pow"
                    | "NotImplemented" | "Ellipsis" | "True" | "False"
                    | "None" | "ValueError" | "TypeError" | "IndexError"
                    | "KeyError" | "RuntimeError" | "Exception"
                    | "BaseException" | "StopIteration" | "KeyboardInterrupt" => {
                        BuiltinDecision::Allow
                    }
                    "open" | "input" => BuiltinDecision::Capability(format!("io.{}", name)),
                    "eval" | "exec" | "compile" => BuiltinDecision::Capability("sys.eval".into()),
                    "__import__" => BuiltinDecision::Capability("sys.import".into()),
                    _ => BuiltinDecision::Allow,
                }
            }
            PythonProfile::External => BuiltinDecision::Allow,
        }
    }
}

/// Whether a builtin is allowed, denied, or requires a capability.
#[derive(Debug, Clone)]
pub enum BuiltinDecision {
    Allow,
    Deny(String),
    Capability(String),
}
