//! `crush-pkg check`: the capabilities a package uses against the ones its
//! `capsule.toml` declares (CRUSH-170).
//!
//! "Used" comes from [`crush_vm::capabilities_used`] over the program
//! `build` compiles (entry + path deps), so it names exactly what the VM
//! will gate on. Ambient capabilities (VM built-ins such as `io.print`,
//! `str.*`, the always-on `caison.parse`, and the pure stdlib; see
//! [`crush_lang_sdk::effects::catalog`]) may be declared but don't have to
//! be. Every other used capability must be covered by an entry in
//! `[capabilities] required` or `optional`. An entry covers a capability
//! when it is:
//!
//! - the same name: `"fs.cat"`;
//! - the same name with a scope: `"fs.read:/var/log"` (the scope isn't
//!   checked);
//! - a family: `"fs"` or `"fs.*"` covers every `fs.<x>`.
//!
//! A `required` entry that covers nothing the program uses is reported as
//! unused. `optional` entries are never reported unused.

use std::collections::BTreeSet;

use crate::manifest::Manifest;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Error,
    Warning,
}

impl Level {
    pub fn as_str(self) -> &'static str {
        match self {
            Level::Error => "error",
            Level::Warning => "warning",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapFinding {
    pub level: Level,
    /// The capability (or `capsule.toml` entry) the finding is about.
    pub capability: String,
    pub message: String,
    pub hint: String,
}

/// The result of checking one package.
#[derive(Debug, Clone, Default)]
pub struct CheckReport {
    /// Every capability the program can request, sorted.
    pub used: BTreeSet<String>,
    pub findings: Vec<CapFinding>,
}

impl CheckReport {
    pub fn has_errors(&self) -> bool {
        self.findings.iter().any(|f| f.level == Level::Error)
    }
}

/// Does `entry` (a `[capabilities]` string) cover capability `cap`? The
/// same rule as `@capabilities` in source (CRUSH-243).
pub use crush_lang_sdk::effects::covers;

/// Compare `used` with `manifest`'s `[capabilities]`, and with its
/// `platforms` (a `web` capsule gets a warning for every capability the
/// browser doesn't provide).
pub fn diff(used: &BTreeSet<String>, manifest: &Manifest) -> Vec<CapFinding> {
    let catalog = crush_lang_sdk::effects::catalog();
    let info = |cap: &str| catalog.iter().find(|c| c.name == cap);
    let caps = &manifest.capabilities;
    let declared = || caps.required.iter().chain(&caps.optional);
    let mut findings = Vec::new();

    for cap in used {
        let ambient = info(cap).is_some_and(|c| c.is_ambient());
        if ambient || declared().any(|entry| covers(entry, cap)) {
            continue;
        }
        let grant = match info(cap) {
            Some(c) => format!("; `{}` grants it at run time", c.grant),
            None if cap.starts_with("polyglot.") => "; `--polyglot` grants it at run time".into(),
            None => String::new(),
        };
        findings.push(CapFinding {
            level: Level::Error,
            capability: cap.clone(),
            message: format!("the program uses `{cap}`, which [capabilities] does not declare"),
            hint: format!("add \"{cap}\" to [capabilities] required{grant}"),
        });
    }

    for entry in &caps.required {
        if !used.iter().any(|cap| covers(entry, cap)) {
            findings.push(CapFinding {
                level: Level::Warning,
                capability: entry.clone(),
                message: format!(
                    "[capabilities] required declares `{entry}`, which the program never uses"
                ),
                hint: format!("remove \"{entry}\" from [capabilities] required"),
            });
        }
    }

    if manifest.capsule.platforms.iter().any(|p| p == "web") {
        // crush-web runs `crush_vm::run` with no host registry, so the
        // browser provides exactly the VM's built-in capabilities.
        let browser = crush_vm::capabilities();
        for cap in used
            .iter()
            .filter(|cap| !browser.contains_key(cap.as_str()))
        {
            findings.push(CapFinding {
                level: Level::Warning,
                capability: cap.clone(),
                message: format!(
                    "[capsule] platforms includes \"web\", but the browser doesn't provide `{cap}`"
                ),
                hint: "the browser runs only the VM built-ins (io.print, str.*, conv.chr/ord, …); \
                       drop \"web\" or stop using this capability"
                    .to_string(),
            });
        }
    }
    findings
}

/// The `capsule.toml` line (1-based) a finding points at: the first line
/// quoting its capability, else the `[capabilities]` header.
pub fn line_of(manifest_text: &str, capability: &str) -> Option<u32> {
    let quoted = format!("\"{capability}\"");
    let lines = || manifest_text.lines();
    lines()
        .position(|line| line.contains(&quoted))
        .or_else(|| lines().position(|line| line.trim() == "[capabilities]"))
        .map(|i| i as u32 + 1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::{CapabilitiesSection, CapsuleSection};

    fn manifest(required: &[&str], optional: &[&str], platforms: &[&str]) -> Manifest {
        Manifest {
            capsule: CapsuleSection {
                name: "t".into(),
                platforms: platforms.iter().map(|s| s.to_string()).collect(),
                ..Default::default()
            },
            capabilities: CapabilitiesSection {
                required: required.iter().map(|s| s.to_string()).collect(),
                optional: optional.iter().map(|s| s.to_string()).collect(),
            },
            ..Default::default()
        }
    }

    fn used(caps: &[&str]) -> BTreeSet<String> {
        caps.iter().map(|s| s.to_string()).collect()
    }

    fn summary(findings: &[CapFinding]) -> Vec<(Level, &str)> {
        findings
            .iter()
            .map(|f| (f.level, f.capability.as_str()))
            .collect()
    }

    #[test]
    fn coverage_rules() {
        assert!(covers("fs.cat", "fs.cat"));
        assert!(covers("fs.read:/var/log", "fs.read"));
        assert!(covers("fs", "fs.cat"));
        assert!(covers("fs.*", "fs.cat"));
        assert!(!covers("fs", "fsx.cat"));
        assert!(!covers("fs.cat", "fs.catalog"));
        assert!(!covers("fs.read", "fs.cat"));
        assert!(!covers("", "fs.cat"));
    }

    #[test]
    fn exact_declaration_is_clean() {
        let f = diff(
            &used(&["fs.cat", "env.get", "io.print"]),
            &manifest(&["fs.cat", "env.get"], &[], &[]),
        );
        assert!(f.is_empty(), "{f:?}");
    }

    #[test]
    fn undeclared_granted_capability_is_an_error_ambient_is_not() {
        let f = diff(
            &used(&["fs.cat", "io.print", "str.split"]),
            &manifest(&[], &[], &[]),
        );
        assert_eq!(summary(&f), [(Level::Error, "fs.cat")]);
        assert!(f[0].hint.contains("--fs"), "{}", f[0].hint);
    }

    #[test]
    fn unknown_and_polyglot_capabilities_must_be_declared() {
        let f = diff(
            &used(&["polyglot.python", "module.load"]),
            &manifest(&[], &[], &[]),
        );
        assert_eq!(
            summary(&f),
            [
                (Level::Error, "module.load"),
                (Level::Error, "polyglot.python")
            ]
        );
        assert!(f[1].hint.contains("--polyglot"));
    }

    #[test]
    fn unused_required_entry_is_a_warning_optional_is_not() {
        let f = diff(
            &used(&["fs.cat"]),
            &manifest(&["fs", "net.http_get", "io.print"], &["time.now"], &[]),
        );
        assert_eq!(
            summary(&f),
            [
                (Level::Warning, "net.http_get"),
                (Level::Warning, "io.print")
            ]
        );
    }

    #[test]
    fn optional_entry_covers_a_used_capability() {
        let f = diff(&used(&["time.now"]), &manifest(&[], &["time"], &[]));
        assert!(f.is_empty(), "{f:?}");
    }

    #[test]
    fn web_platform_warns_for_capabilities_the_browser_lacks() {
        let f = diff(
            &used(&["io.print", "str.len", "fs.cat"]),
            &manifest(&["fs.cat"], &[], &["linux", "web"]),
        );
        assert_eq!(summary(&f), [(Level::Warning, "fs.cat")]);
        assert!(f[0].message.contains("web"));
        // Not web: no warning.
        assert!(diff(&used(&["fs.cat"]), &manifest(&["fs.cat"], &[], &["linux"])).is_empty());
    }

    #[test]
    fn line_of_finds_the_quoted_entry() {
        let text =
            "[capsule]\nname = \"x\"\n\n[capabilities]\nrequired = [\n  \"fs\",\n  \"net\",\n]\n";
        assert_eq!(line_of(text, "net"), Some(7));
        assert_eq!(line_of(text, "time"), Some(4));
        assert_eq!(line_of("[capsule]\n", "time"), None);
    }
}
