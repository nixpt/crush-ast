//! Source spans and validation diagnostics.

use serde::{Deserialize, Serialize};
use std::fmt;

/// Where a declaration came from in its home-language source.
///
/// Optional on every declaration (like CAST's `meta` line/col): walkers fill
/// it when they know it, and diagnostics carry it through. Lines and columns
/// are 1-based.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Span {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    pub line: u32,
    pub col: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_line: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_col: Option<u32>,
}

impl Span {
    /// A point span at `line:col` with no file.
    pub fn new(line: u32, col: u32) -> Self {
        Span { file: None, line, col, end_line: None, end_col: None }
    }

    /// The same span, attributed to `file`.
    pub fn with_file(mut self, file: impl Into<String>) -> Self {
        self.file = Some(file.into());
        self
    }

    /// The same span, extended to end at `end_line:end_col`.
    pub fn with_end(mut self, end_line: u32, end_col: u32) -> Self {
        self.end_line = Some(end_line);
        self.end_col = Some(end_col);
        self
    }
}

impl fmt::Display for Span {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(file) = &self.file {
            write!(f, "{file}:")?;
        }
        write!(f, "{}:{}", self.line, self.col)
    }
}

/// What a [`Diagnostic`] is about. Closed and stable: tools may match on it,
/// and [`DiagnosticCode::as_str`] is the stable spelling (`cosm::<name>`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticCode {
    /// Two declarations (kinds and/or interfaces) in one module share a name.
    DuplicateDecl,
    /// A `parents` entry names no kind in the module or its dependencies.
    UnknownParent,
    /// A `parents` entry names an interface, not a kind.
    ParentIsInterface,
    /// A `conforms` entry names no interface in the module or its dependencies.
    UnknownInterface,
    /// A `conforms` entry names a kind, not an interface.
    ConformsToKind,
    /// A kind declares conformance to the same interface twice.
    DuplicateConformance,
    /// Two sealed fields (or slots) share a name.
    DuplicateField,
    /// Two members of one kind, conformance or interface share a name.
    DuplicateMember,
    /// A method, accessor or static member has the same name as a sealed field.
    FieldShadowedByMember,
    /// A `field` member on a sealed kind names no declared field.
    UndeclaredField,
    /// An accessor with neither `get` nor `set`.
    EmptyAccessor,
    /// `value(copy)` / `value(move)` combined with open (`open_dict*`) storage.
    ValueSemanticsWithOpenStorage,
    /// The number of parents doesn't fit the resolution strategy
    /// (`none` with parents, `single`/`prototype_chain` with more than one).
    ParentArity,
    /// A parent is listed twice (Python: `duplicate base class X`).
    DuplicateBase,
    /// C3 cannot linearize the hierarchy (Python: `Cannot create a consistent
    /// method resolution order (MRO) for bases …`).
    InconsistentMro,
    /// A kind inherits (transitively) from itself.
    InheritanceCycle,
    /// A parent's resolution strategy has no linear order to inherit
    /// (an `embedding_promotion` kind used as a `c3`/`single` parent).
    IncompatibleParent,
    /// A conformance's `check` differs from the interface's `satisfaction`.
    SatisfactionMismatch,
    /// An interface member is not provided by the conformance, the kind (or
    /// an ancestor), or an interface default.
    MissingInterfaceMember,
    /// A conformance provides a member the interface doesn't declare.
    UnknownInterfaceMember,
}

impl DiagnosticCode {
    /// Stable spelling, e.g. `cosm::inconsistent_mro`.
    pub fn as_str(self) -> &'static str {
        use DiagnosticCode::*;
        match self {
            DuplicateDecl => "cosm::duplicate_decl",
            UnknownParent => "cosm::unknown_parent",
            ParentIsInterface => "cosm::parent_is_interface",
            UnknownInterface => "cosm::unknown_interface",
            ConformsToKind => "cosm::conforms_to_kind",
            DuplicateConformance => "cosm::duplicate_conformance",
            DuplicateField => "cosm::duplicate_field",
            DuplicateMember => "cosm::duplicate_member",
            FieldShadowedByMember => "cosm::field_shadowed_by_member",
            UndeclaredField => "cosm::undeclared_field",
            EmptyAccessor => "cosm::empty_accessor",
            ValueSemanticsWithOpenStorage => "cosm::value_semantics_with_open_storage",
            ParentArity => "cosm::parent_arity",
            DuplicateBase => "cosm::duplicate_base",
            InconsistentMro => "cosm::inconsistent_mro",
            InheritanceCycle => "cosm::inheritance_cycle",
            IncompatibleParent => "cosm::incompatible_parent",
            SatisfactionMismatch => "cosm::satisfaction_mismatch",
            MissingInterfaceMember => "cosm::missing_interface_member",
            UnknownInterfaceMember => "cosm::unknown_interface_member",
        }
    }
}

impl fmt::Display for DiagnosticCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One validation finding. Every COSM diagnostic is an error: a module with
/// any diagnostics must not be loaded.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diagnostic {
    pub code: DiagnosticCode,
    /// The declaration the finding is about: `Kind`, `Kind.member`,
    /// `Kind: Interface`, `Kind: Interface.member` or `Interface.member`.
    pub subject: String,
    pub message: String,
    /// The most specific span known (member, conformance, field, then the
    /// declaration itself). `None` when the producer recorded no spans.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span: Option<Span>,
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(span) = &self.span {
            write!(f, "{span}: ")?;
        }
        write!(f, "error[{}]: {}: {}", self.code, self.subject, self.message)
    }
}
