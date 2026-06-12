//! Debug information for source-level debugging.
//!
//! This module provides types for mapping between source code locations,
//! CAST AST nodes, and CASM bytecode instructions.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Per-instruction source location.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceLocation {
    /// 1-indexed source line.
    pub line: u32,
    /// 1-indexed source column.
    pub col: u32,
    /// Optional source file path.
    #[serde(default)]
    pub file: Option<String>,
}

impl SourceLocation {
    pub fn new(line: u32, col: u32, file: Option<String>) -> Self {
        Self { line, col, file }
    }
}

/// Source file information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceFile {
    /// File path (relative or absolute)
    pub path: String,
    /// File content hash for verification
    pub content_hash: Option<String>,
    /// Source language
    pub language: Option<String>,
}

/// A location in source code
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SourceSpan {
    /// Source file index (into DebugInfo.sources)
    pub file_idx: usize,
    /// Start line (1-indexed)
    pub start_line: u32,
    /// Start column (1-indexed)
    pub start_col: u32,
    /// End line (1-indexed)
    pub end_line: u32,
    /// End column (1-indexed)
    pub end_col: u32,
}

impl SourceSpan {
    pub fn new(
        file_idx: usize,
        start_line: u32,
        start_col: u32,
        end_line: u32,
        end_col: u32,
    ) -> Self {
        Self {
            file_idx,
            start_line,
            start_col,
            end_line,
            end_col,
        }
    }

    /// Create a single-line span
    pub fn line(file_idx: usize, line: u32) -> Self {
        Self {
            file_idx,
            start_line: line,
            start_col: 1,
            end_line: line,
            end_col: u32::MAX,
        }
    }
}

/// A mapping entry from CASM instruction to source location
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MappingEntry {
    /// Function name
    pub function: String,
    /// Instruction index within function
    pub pc: usize,
    /// Source span
    pub span: SourceSpan,
    /// CAST node ID (if available)
    pub cast_node_id: Option<u32>,
    /// Whether this is a statement boundary (for stepping)
    pub is_statement: bool,
}

/// Debug information for a CASM program
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DebugInfo {
    /// Source files referenced
    pub sources: Vec<SourceFile>,
    /// Instruction-to-source mappings
    pub mappings: Vec<MappingEntry>,
    /// Compiler/walker version that generated this
    pub compiler_version: Option<String>,
    /// Original program name
    pub program_name: Option<String>,
    /// Linear source map indexed by emitted instruction index.
    #[serde(default)]
    pub source_map: Vec<SourceLocation>,
}

impl DebugInfo {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a source file and return its index
    pub fn add_source(&mut self, path: impl Into<String>, language: Option<String>) -> usize {
        let idx = self.sources.len();
        self.sources.push(SourceFile {
            path: path.into(),
            content_hash: None,
            language,
        });
        idx
    }

    /// Add a mapping entry
    pub fn add_mapping(&mut self, entry: MappingEntry) {
        self.mappings.push(entry);
    }

    /// Add a simple mapping
    pub fn map_instruction(
        &mut self,
        function: impl Into<String>,
        pc: usize,
        file_idx: usize,
        line: u32,
        col: u32,
    ) {
        self.mappings.push(MappingEntry {
            function: function.into(),
            pc,
            span: SourceSpan::new(file_idx, line, col, line, col),
            cast_node_id: None,
            is_statement: false,
        });
    }

    /// Find source location for a given instruction
    pub fn find_source(&self, function: &str, pc: usize) -> Option<(&SourceFile, &SourceSpan)> {
        self.mappings
            .iter()
            .find(|m| m.function == function && m.pc == pc)
            .and_then(|m| self.sources.get(m.span.file_idx).map(|s| (s, &m.span)))
    }

    /// Append a source location for the next emitted instruction.
    pub fn push_source_location(&mut self, loc: SourceLocation) {
        self.source_map.push(loc);
    }

    /// Lookup source location by linear program-counter index.
    pub fn source_location_for_pc(&self, pc: usize) -> Option<&SourceLocation> {
        self.source_map.get(pc)
    }

    /// Find all instructions for a given source line
    pub fn find_instructions(&self, file_idx: usize, line: u32) -> Vec<(&str, usize)> {
        self.mappings
            .iter()
            .filter(|m| {
                m.span.file_idx == file_idx && m.span.start_line <= line && m.span.end_line >= line
            })
            .map(|m| (m.function.as_str(), m.pc))
            .collect()
    }

    /// Find the first statement boundary at or after a given line
    pub fn find_statement_at_line(&self, file_idx: usize, line: u32) -> Option<(&str, usize)> {
        self.mappings
            .iter()
            .filter(|m| m.is_statement && m.span.file_idx == file_idx && m.span.start_line >= line)
            .min_by_key(|m| m.span.start_line)
            .map(|m| (m.function.as_str(), m.pc))
    }

    /// Build a line-to-instruction index for fast lookup
    pub fn build_line_index(&self) -> HashMap<(usize, u32), Vec<(String, usize)>> {
        let mut index: HashMap<(usize, u32), Vec<(String, usize)>> = HashMap::new();
        for m in &self.mappings {
            for line in m.span.start_line..=m.span.end_line {
                index
                    .entry((m.span.file_idx, line))
                    .or_default()
                    .push((m.function.clone(), m.pc));
            }
        }
        index
    }

    /// Serialize to JSON
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    /// Deserialize from JSON
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    /// Save to a .dbg sidecar file
    pub fn save(&self, path: &std::path::Path) -> std::io::Result<()> {
        let json = self
            .to_json()
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        std::fs::write(path, json)
    }

    /// Load from a .dbg sidecar file
    pub fn load(path: &std::path::Path) -> std::io::Result<Self> {
        let json = std::fs::read_to_string(path)?;
        Self::from_json(&json).map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    }
}

/// Extract debug info from CASM instruction metadata
pub fn extract_from_meta(meta: &serde_json::Value) -> Option<(u32, u32)> {
    let line = meta.get("line")?.as_u64()? as u32;
    let col = meta.get("col").and_then(|v| v.as_u64()).unwrap_or(1) as u32;
    Some((line, col))
}

/// Extract source location from instruction metadata.
pub fn extract_source_location(meta: &serde_json::Value) -> Option<SourceLocation> {
    let (line, col) = extract_from_meta(meta)?;
    let file = meta
        .get("file")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    Some(SourceLocation::new(line, col, file))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_debug_info_basic() {
        let mut info = DebugInfo::new();
        let file_idx = info.add_source("main.py", Some("python".to_string()));

        info.map_instruction("main", 0, file_idx, 1, 1);
        info.map_instruction("main", 1, file_idx, 2, 5);
        info.map_instruction("main", 2, file_idx, 3, 1);

        assert_eq!(info.sources.len(), 1);
        assert_eq!(info.mappings.len(), 3);

        let (source, span) = info.find_source("main", 1).unwrap();
        assert_eq!(source.path, "main.py");
        assert_eq!(span.start_line, 2);
    }

    #[test]
    fn test_find_instructions() {
        let mut info = DebugInfo::new();
        let file_idx = info.add_source("test.py", None);

        info.map_instruction("foo", 0, file_idx, 10, 1);
        info.map_instruction("foo", 1, file_idx, 10, 5);
        info.map_instruction("foo", 2, file_idx, 11, 1);

        let instrs = info.find_instructions(file_idx, 10);
        assert_eq!(instrs.len(), 2);
    }

    #[test]
    fn test_json_roundtrip() {
        let mut info = DebugInfo::new();
        info.add_source("test.rs", Some("rust".to_string()));
        info.map_instruction("main", 0, 0, 1, 1);
        info.push_source_location(SourceLocation::new(1, 1, Some("test.rs".to_string())));
        info.compiler_version = Some("0.1.0".to_string());

        let json = info.to_json().unwrap();
        let loaded = DebugInfo::from_json(&json).unwrap();

        assert_eq!(loaded.sources.len(), 1);
        assert_eq!(loaded.mappings.len(), 1);
        assert_eq!(loaded.source_map.len(), 1);
        assert_eq!(loaded.compiler_version, Some("0.1.0".to_string()));
    }

    #[test]
    fn test_source_location_for_pc() {
        let mut info = DebugInfo::new();
        info.push_source_location(SourceLocation::new(10, 2, Some("main.crush".to_string())));
        info.push_source_location(SourceLocation::new(11, 7, Some("main.crush".to_string())));

        let loc = info.source_location_for_pc(1).unwrap();
        assert_eq!(loc.line, 11);
        assert_eq!(loc.col, 7);
        assert_eq!(loc.file.as_deref(), Some("main.crush"));
        assert!(info.source_location_for_pc(2).is_none());
    }
}
