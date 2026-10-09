//! Effect declarations for the capabilities [`HostCapsBuilder`] registers
//! (CRUSH-155).
//!
//! [`HostCap::effects`] defaults to `None` ("undeclared") so the trait change
//! costs existing implementations nothing. Rather than override it in every
//! one of the SDK's capability types, the builder wraps each handler it
//! registers with the labels from [`effects_of`] — one table, next to the
//! grant that unlocks each family. The whole stdlib is pure (`[]`) and is
//! tagged as a family when registered.
//!
//! Labels are `"<resource>/<action>"`. They describe what a call touches
//! outside the VM; they never decide whether it is allowed — grants do that.
//!
//! [`HostCapsBuilder`]: crate::HostCapsBuilder

use std::sync::Arc;

use crush_vm::host::HostCapError;
use crush_vm::vm::Value;
use crush_vm::{HostCap, HostCapSpec, HostCaps};

/// The effects of a builder-registered, non-stdlib capability, or `None`
/// for a name this table does not know.
pub fn effects_of(name: &str) -> Option<&'static [&'static str]> {
    const FS_READ: &[&str] = &["fs/read"];
    const FS_WRITE: &[&str] = &["fs/write"];
    let effects: &'static [&'static str] = match name {
        // Always registered: parses its argument, nothing else. `cson.parse`
        // is the deprecated alias (CRUSH-149).
        "caison.parse" | "cson.parse" => &[],
        // Always registered: the program's own arguments, and ending the program.
        "sys.args" | "sys.exit" => &[],
        // --fs
        "fs.read" | "fs.cat" | "fs.exists" | "fs.list" | "fs.ls" | "fs.find" | "fs.pwd"
        | "fs.cd" => FS_READ,
        "fs.write" | "fs.mkdir" | "fs.rm" | "fs.mv" | "fs.touch" => FS_WRITE,
        "fs.cp" => &["fs/read", "fs/write"],
        "text.head" | "text.tail" | "text.wc" | "text.cut" | "text.grep" => FS_READ,
        // --env
        "env.get" | "env.all" | "env.home_dir" => &["env/read"],
        // --time
        "time.now" | "time.now_ms" | "time.now_iso" | "time.elapsed" => &["time/read"],
        "time.sleep" | "async.sleep" => &["time/sleep"],
        // --bus (in-process broker)
        "message_bus.publish" => &["bus/publish"],
        "message_bus.subscribe" => &["bus/subscribe"],
        "message_bus.recv" => &["bus/recv"],
        // --task / --process
        "task.start" | "process.exec" => &["process/spawn"],
        "task.stop" => &["process/kill"],
        "task.list" => &["process/read"],
        // --akg (in-process store)
        "akg.write" => &["akg/write"],
        "akg.read" | "akg.search" => &["akg/read"],
        // --crypto
        "crypto.sha256" => &[],
        "crypto.random" => &["crypto/random"],
        // --graphics: an in-memory canvas
        n if n.starts_with("graphics.") => &[],
        // --net
        n if n.starts_with("net.") => &["net/http"],
        // --db
        "db.query" => &["db/read"],
        "db.execute" => &["db/write"],
        // codebase index handed in by the host
        n if n.starts_with("codebase.") => &["codebase/read"],
        // ai_native.* — echo stubs today, model calls later
        n if n.starts_with("ai_native.") => &["ai/call"],
        _ => return None,
    };
    Some(effects)
}

/// One capability this build can register: its spec, effects, and the
/// grant that unlocks it. See [`catalog`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapInfo {
    pub name: String,
    pub argc: Option<usize>,
    pub returns: bool,
    pub effects: Vec<&'static str>,
    /// `"portable"` (VM built-in), `"always"` (registered by every
    /// [`HostCapsBuilder`](crate::HostCapsBuilder)), the stdlib, or the
    /// `crush run` flag that grants it (`"--fs"`, `"--polyglot"`, …).
    pub grant: &'static str,
}

/// The `grant` of the pure standard library in [`catalog`].
pub const STDLIB_GRANT: &str = "stdlib (default; --no-stdlib)";

impl CapInfo {
    /// Available without any grant: a VM built-in, always registered, or the
    /// pure stdlib. Everything else needs a grant from whoever runs the
    /// program.
    pub fn is_ambient(&self) -> bool {
        matches!(self.grant, "portable" | "always" | STDLIB_GRANT)
    }
}

/// Every capability this build can register, sorted by name, with its
/// effects and the grant that unlocks it. A name appears once, under the
/// first grant that registers it (built-ins, then always, then stdlib, then
/// the flags). `crush-run caps --json` prints this; `crush-pkg check` uses it
/// to tell ambient capabilities from granted ones.
pub fn catalog() -> Vec<CapInfo> {
    use crate::HostCapsBuilder;
    let new = HostCapsBuilder::new;
    #[allow(unused_mut)]
    let mut grants: Vec<(&'static str, HostCapsBuilder)> = vec![("always", new())];
    #[cfg(feature = "stdlib")]
    grants.push((STDLIB_GRANT, new().stdlib(true)));
    grants.extend([
        ("--fs", new().fs(true)),
        ("--env", new().env(true)),
        ("--time", new().time(true)),
        ("--bus", new().bus(true)),
        ("--task", new().task(true)),
        ("--akg", new().akg(true)),
        ("--process", new().process(true)),
        ("--crypto", new().crypto(true)),
        ("--polyglot", new().polyglot(&["python", "javascript", "bash"])),
    ]);
    #[cfg(feature = "graphics")]
    grants.push(("--graphics", new().graphics(true)));
    #[cfg(feature = "net")]
    grants.push(("--net", new().net(true)));
    #[cfg(feature = "db")]
    grants.push(("--db PATH", new().db(":memory:")));

    let mut out = std::collections::BTreeMap::new();
    for spec in crush_vm::capabilities().values() {
        let effects: &[&str] = match spec.name {
            "io.print" => &["stdout/write"],
            "io.read" => &["stdin/read"],
            _ => &[],
        };
        out.insert(
            spec.name.to_string(),
            CapInfo {
                name: spec.name.to_string(),
                argc: spec.argc,
                returns: spec.returns,
                effects: effects.to_vec(),
                grant: "portable",
            },
        );
    }
    for (grant, builder) in grants {
        let caps = builder.build();
        for name in caps.names() {
            if out.contains_key(name) {
                continue;
            }
            let cap = caps.get(name).expect("listed name");
            let spec = cap.spec();
            out.insert(
                name.to_string(),
                CapInfo {
                    name: spec.name.clone(),
                    argc: spec.argc,
                    returns: spec.returns,
                    effects: cap.effects().unwrap_or_default().to_vec(),
                    grant,
                },
            );
        }
    }
    out.into_values().collect()
}

/// Does `entry` (a declared capability) cover capability `cap`? An entry is
/// the same name (`fs.cat`), the same name with a scope (`fs.read:/var/log`;
/// the scope isn't checked), or a family (`fs` or `fs.*` covers every
/// `fs.<x>`). Shared by `@capabilities` (CRUSH-232) and `crush-pkg check`'s
/// `capsule.toml` (CRUSH-170).
pub fn covers(entry: &str, cap: &str) -> bool {
    let base = entry.split(':').next().unwrap_or(entry).trim();
    let family = base.strip_suffix(".*").unwrap_or(base);
    base == cap
        || (!family.is_empty()
            && cap.len() > family.len()
            && cap.starts_with(family)
            && cap.as_bytes()[family.len()] == b'.')
}

/// The capabilities `program` can request: every `CAP_CALL` and gated opcode
/// reachable from its entry ([`crush_vm::capabilities_used`]), sorted.
pub fn used_by(program: &crush_vm::Program) -> anyhow::Result<std::collections::BTreeSet<String>> {
    crush_vm::capabilities_used(program).map_err(|e| anyhow::anyhow!("capability inference: {e}"))
}

/// The capabilities in `used` that need a grant (not ambient, see
/// [`CapInfo::is_ambient`]) and that no entry of `declared` covers.
pub fn undeclared(used: &std::collections::BTreeSet<String>, declared: &[String]) -> Vec<String> {
    let catalog = catalog();
    used.iter()
        .filter(|cap| !catalog.iter().any(|c| &c.name == *cap && c.is_ambient()))
        .filter(|cap| !declared.iter().any(|entry| covers(entry, cap)))
        .cloned()
        .collect()
}

/// A capability a program can request that the host running it doesn't grant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MissingGrant {
    pub capability: String,
    /// What grants it in `crush run` terms (`"--fs"`, `"--time"`, …), or
    /// `None` when no grant in this build provides it.
    pub grant: Option<&'static str>,
}

/// Every capability `program` can request that is neither a VM built-in nor
/// registered in `host_caps` (CRUSH-232). A host calls this before running
/// agent-written code so a missing grant is refused up front, with the whole
/// list, instead of failing mid-run after earlier effects happened.
///
/// Inference is static: a capability only reachable through a function name
/// computed at run time (`spawn`) isn't seen, and the VM's own check still
/// applies when the program runs.
pub fn missing_grants(program: &crush_vm::Program, host_caps: Option<&HostCaps>) -> anyhow::Result<Vec<MissingGrant>> {
    let builtins = crush_vm::capabilities();
    let missing: Vec<String> = used_by(program)?
        .into_iter()
        .filter(|cap| !builtins.contains_key(cap.as_str()))
        .filter(|cap| host_caps.is_none_or(|h| h.get(cap).is_none()))
        .collect();
    if missing.is_empty() {
        return Ok(Vec::new());
    }
    let catalog = catalog();
    Ok(missing
        .into_iter()
        .map(|capability| {
            let grant = catalog.iter().find(|c| c.name == capability).map(|c| c.grant);
            MissingGrant { capability, grant }
        })
        .collect())
}

/// One line per missing grant, grouped by what grants it:
/// `--fs: fs.cat, fs.list`.
pub fn describe_missing(missing: &[MissingGrant]) -> String {
    let mut groups: std::collections::BTreeMap<&str, Vec<&str>> = std::collections::BTreeMap::new();
    for m in missing {
        groups
            .entry(m.grant.unwrap_or("not available in this host"))
            .or_default()
            .push(&m.capability);
    }
    groups
        .iter()
        .map(|(grant, caps)| format!("  {grant}: {}", caps.join(", ")))
        .collect::<Vec<_>>()
        .join("\n")
}

/// A handler plus its declared effects; everything else is delegated.
struct Declared {
    inner: Arc<dyn HostCap>,
    effects: &'static [&'static str],
}

impl HostCap for Declared {
    fn spec(&self) -> HostCapSpec {
        self.inner.spec()
    }

    fn call(&self, args: Vec<Value>) -> Result<Option<Value>, String> {
        self.inner.call(args)
    }

    fn call_with_deadline(
        &self,
        args: Vec<Value>,
        deadline_ms: u64,
    ) -> Result<Option<Value>, HostCapError> {
        self.inner.call_with_deadline(args, deadline_ms)
    }

    fn effects(&self) -> Option<&'static [&'static str]> {
        Some(self.effects)
    }
}

/// Move every handler of `from` into `into`, declaring `effects` for each
/// one that has not declared its own.
pub(crate) fn register_all(into: &mut HostCaps, from: HostCaps, effects: &'static [&'static str]) {
    for handler in from.into_handlers() {
        into.register_shared(with_effects(handler, Some(effects)));
    }
}

/// Re-register every handler, declaring effects from [`effects_of`] where the
/// handler has none of its own. Unknown names stay undeclared.
pub(crate) fn declare(caps: HostCaps) -> HostCaps {
    let mut out = HostCaps::new();
    for handler in caps.into_handlers() {
        let effects = effects_of(&handler.spec().name);
        out.register_shared(with_effects(handler, effects));
    }
    out
}

fn with_effects(
    handler: Arc<dyn HostCap>,
    effects: Option<&'static [&'static str]>,
) -> Arc<dyn HostCap> {
    match (handler.effects(), effects) {
        (None, Some(effects)) => Arc::new(Declared {
            inner: handler,
            effects,
        }),
        _ => handler,
    }
}

#[cfg(test)]
mod tests {
    use crate::HostCapsBuilder;

    #[test]
    fn undeclared_ignores_ambient_and_honours_families() {
        use std::collections::BTreeSet;
        let used: BTreeSet<String> =
            ["io.print", "str.len", "fs.cat", "fs.list", "time.now", "env.get"].map(String::from).into();
        let declared = vec!["fs".to_string(), "time.now:anything".to_string()];
        assert_eq!(super::undeclared(&used, &declared), ["env.get"]);
        assert_eq!(super::undeclared(&used, &[]), ["env.get", "fs.cat", "fs.list", "time.now"]);
    }

    #[test]
    fn missing_grants_names_what_grants_each() {
        let program = crate::compile::compile_crush_source(
            "fn main() { let t = time.now(); io.print(fs.cat(\"x\")); return 0 }",
        )
        .unwrap();
        let none = super::missing_grants(&program, None).unwrap();
        let names: Vec<_> = none.iter().map(|m| (m.capability.as_str(), m.grant)).collect();
        assert_eq!(names, [("fs.cat", Some("--fs")), ("time.now", Some("--time"))]);
        let granted = HostCapsBuilder::new().fs(true).time(true).build();
        assert!(super::missing_grants(&program, Some(&granted)).unwrap().is_empty());
        assert_eq!(
            super::describe_missing(&none),
            "  --fs: fs.cat\n  --time: time.now"
        );
    }

    /// Every grant at once — the most a builder can register.
    fn everything() -> crush_vm::HostCaps {
        #[allow(unused_mut)]
        let mut b = HostCapsBuilder::new()
            .fs(true)
            .env(true)
            .time(true)
            .bus(true)
            .task(true)
            .akg(true)
            .process(true)
            .crypto(true)
            .ai_native(true)
            .polyglot(&["python", "bash"]);
        #[cfg(feature = "stdlib")]
        {
            b = b.stdlib(true);
        }
        #[cfg(feature = "graphics")]
        {
            b = b.graphics(true);
        }
        #[cfg(feature = "net")]
        {
            b = b.net(true);
        }
        #[cfg(feature = "db")]
        {
            b = b.db(":memory:");
        }
        b.build()
    }

    // Drift guard: a new capability the builder registers must be added to
    // `effects_of` (or declare its own) — otherwise it shows up here.
    #[test]
    fn every_builder_capability_declares_its_effects() {
        let caps = everything();
        let mut undeclared: Vec<_> = caps
            .names()
            .filter(|n| caps.get(n).unwrap().effects().is_none())
            .collect();
        undeclared.sort();
        assert!(undeclared.is_empty(), "undeclared effects: {undeclared:?}");
        assert!(caps.names().count() > 40);
    }

    #[test]
    fn effects_name_what_a_capability_touches() {
        let caps = everything();
        let effects = |n: &str| caps.get(n).unwrap().effects().unwrap().to_vec();
        assert_eq!(effects("fs.read"), ["fs/read"]);
        assert_eq!(effects("fs.cp"), ["fs/read", "fs/write"]);
        assert_eq!(effects("env.all"), ["env/read"]);
        assert_eq!(effects("time.sleep"), ["time/sleep"]);
        assert_eq!(effects("async.sleep"), ["time/sleep"]);
        assert_eq!(effects("process.exec"), ["process/spawn"]);
        assert_eq!(effects("polyglot.python"), ["process/spawn"]);
        assert!(effects("caison.parse").is_empty());
        assert!(effects("cson.parse").is_empty());
        #[cfg(feature = "stdlib")]
        {
            assert!(effects("conv.to_str").is_empty());
            assert!(effects("system.path_normalize").is_empty());
        }
        #[cfg(feature = "net")]
        assert_eq!(effects("net.http_request"), ["net/http"]);
    }

    #[test]
    fn catalog_names_each_capability_once_with_its_grant() {
        let catalog = super::catalog();
        let get = |n: &str| catalog.iter().find(|c| c.name == n).unwrap_or_else(|| panic!("{n}"));
        assert!(get("io.print").is_ambient());
        assert!(get("push").is_ambient());
        assert!(get("caison.parse").is_ambient());
        assert_eq!(get("fs.cat").grant, "--fs");
        assert!(!get("fs.cat").is_ambient());
        assert_eq!(get("env.get").effects, ["env/read"]);
        assert_eq!(get("polyglot.python").grant, "--polyglot");
        #[cfg(feature = "stdlib")]
        assert!(get("conv.to_str").is_ambient());
        let names: Vec<_> = catalog.iter().map(|c| c.name.as_str()).collect();
        let mut sorted = names.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(names, sorted, "sorted, no duplicates");
    }

    #[test]
    fn declaring_effects_does_not_change_behaviour() {
        let caps = HostCapsBuilder::new()
            .env(true)
            .with_env_var("K", "v")
            .build();
        let cap = caps.get("env.get").unwrap();
        assert_eq!(cap.spec().name, "env.get");
        assert_eq!(
            cap.call(vec![crush_vm::vm::Value::Str("K".into())]),
            Ok(Some(crush_vm::vm::Value::Str("v".into())))
        );
    }
}
