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
        "fs.read" | "fs.cat" | "fs.exists" | "fs.is_dir" | "fs.list" | "fs.ls" | "fs.find" | "fs.pwd"
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
        // --gpu: runs code on the GPU
        "gpu.launch" => &["gpu/launch"],
        // --graphics: an in-memory canvas
        n if n.starts_with("graphics.") => &[],
        // --tcp / --tcp-listen: raw sockets (CRUSH-318). Deliberately not
        // `net/*`, so nothing that allows HTTP allows these.
        "tcp.connect" | "tcp.tls_wrap" => &["tcp/connect"],
        "tcp.listen" | "tcp.accept" => &["tcp/listen"],
        "tcp.send" | "tcp.recv" | "tcp.close" => &["tcp/io"],
        "tcp.ping" => &[],
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

/// What a refusal names for a `ui.*` capability in a host that has none
/// (`crush-run`): they are provided by `crush gui` / `crush-gui` (CRUSH-319).
const UI_GRANT: &str = "--ui (run it with `crush gui run`)";

/// The `grant` of the pure standard library in [`catalog`].
pub const STDLIB_GRANT: &str = "stdlib (default; --no-stdlib)";

/// The `grant` of `tcp.send`/`recv`/`close`/`ping` in [`catalog`]: either flag.
pub const TCP_IO_GRANT: &str = "--tcp or --tcp-listen";

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
    #[cfg(feature = "gpu")]
    grants.push(("--gpu", new().gpu(true)));
    #[cfg(feature = "graphics")]
    grants.push(("--graphics", new().graphics(true)));
    #[cfg(feature = "net")]
    grants.push(("--net", new().net(true)));
    #[cfg(feature = "tcp")]
    grants.extend([
        ("--tcp", new().tcp(true)),
        ("--tcp-listen", new().tcp_listen(true)),
    ]);
    #[cfg(feature = "db")]
    grants.push(("--db PATH", new().db(":memory:")));
    let mut extra: Vec<(&'static str, HostCaps)> = Vec::new();

    // `--codebase`: registered with a fixed date, not via the builder, whose
    // `register` reads the wall clock (`Utc::now()` panics on wasm32, and the
    // catalog is built at startup by crush-web).
    {
        let mut caps = crush_vm::HostCaps::new();
        crate::codebase::register_at(
            &mut caps,
            Arc::new(crush_index::CrushIndex::new()),
            chrono::NaiveDate::from_ymd_opt(2000, 1, 1).expect("valid date"),
        );
        extra.push(("--codebase FILE", declare(caps)));
    }

    let mut out = std::collections::BTreeMap::new();
    for spec in crush_vm::capabilities().values() {
        let effects: &[&str] = match spec.name {
            "io.print" => &["stdout/write"],
            "io.read" | "io.read_line" => &["stdin/read"],
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
    let built = grants.into_iter().map(|(g, b)| (g, b.build())).chain(extra);
    for (grant, caps) in built {
        for name in caps.names() {
            if out.contains_key(name) {
                continue;
            }
            // Either tcp grant unlocks the connection-id operations.
            let grant = if matches!(name, "tcp.send" | "tcp.recv" | "tcp.close" | "tcp.ping") {
                TCP_IO_GRANT
            } else {
                grant
            };
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
/// `fs.<x>`). Shared by `@capabilities` (CRUSH-243) and `crush-pkg check`'s
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
    /// Set when `capability` is a name from another runtime that this one
    /// renamed (CRUSH-264): the replacement, e.g. `net.http_get` for
    /// `http.get`. Such a program can never run as written, whatever is granted.
    pub renamed_to: Option<&'static str>,
}

/// Capability names from exosphere's stdlib that crush-ast renamed, with no
/// alias (CRUSH-264): `(old, new)`. A program that calls the old name is
/// refused up front, naming the replacement, by every runner that preflights
/// with [`missing_grants`].
pub const RENAMED_CAPS: &[(&str, &str)] = &[
    ("http.get", "net.http_get"),
    ("http.post", "net.http_post"),
    ("http.put", "net.http_put"),
    ("http.delete", "net.http_delete"),
    ("http.request", "net.http_request"),
];

/// The replacement for a renamed capability, if `cap` is one.
pub fn renamed_to(cap: &str) -> Option<&'static str> {
    RENAMED_CAPS
        .iter()
        .find(|(old, _)| *old == cap)
        .map(|(_, new)| *new)
}

/// Every capability `program` can request that is neither a VM built-in nor
/// registered in `host_caps` (CRUSH-243). A host calls this before running
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
            let grant = catalog
                .iter()
                .find(|c| c.name == capability)
                .map(|c| c.grant)
                // `ui.*` lives in the crush-gui crate (it needs arniko), so
                // the catalog cannot list it; say where it is granted.
                .or_else(|| capability.starts_with("ui.").then_some(UI_GRANT));
            let renamed_to = renamed_to(&capability);
            MissingGrant {
                capability,
                grant,
                renamed_to,
            }
        })
        .collect())
}

/// One line per missing grant, grouped by what grants it:
/// `--fs: fs.cat, fs.list`.
pub fn describe_missing(missing: &[MissingGrant]) -> String {
    let mut groups: std::collections::BTreeMap<&str, Vec<&str>> = std::collections::BTreeMap::new();
    let mut renamed = Vec::new();
    for m in missing {
        if let Some(new) = m.renamed_to {
            renamed.push(format!(
                "  {} was renamed to {new}{}",
                m.capability,
                grant_hint(new)
            ));
            continue;
        }
        groups
            .entry(m.grant.unwrap_or("not available in this host"))
            .or_default()
            .push(&m.capability);
    }
    renamed
        .into_iter()
        .chain(
            groups
                .iter()
                .map(|(grant, caps)| format!("  {grant}: {}", caps.join(", "))),
        )
        .collect::<Vec<_>>()
        .join("\n")
}

/// ` (grant it with --net)` for a replacement name, or `""` when this build
/// has no grant for it.
fn grant_hint(cap: &str) -> String {
    match catalog().iter().find(|c| c.name == cap) {
        Some(c) => format!(" (grant it with {})", c.grant),
        // Built without the feature that provides it: `net` is a default
        // feature, so this is a `default-features = false` embedder.
        None => " (not available in this build)".to_string(),
    }
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
pub(crate) fn register_all(into: &mut HostCaps, mut from: HostCaps, effects: &'static [&'static str]) {
    // Not a handler; keep it unless `into` already has one (CRUSH-271).
    if let Some(executor) = from.take_polyglot_executor()
        && into.polyglot_executor().is_none()
    {
        into.set_polyglot_executor(executor);
    }
    for handler in from.into_handlers() {
        into.register_shared(with_effects(handler, Some(effects)));
    }
}

/// Re-register every handler, declaring effects from [`effects_of`] where the
/// handler has none of its own. Unknown names stay undeclared.
pub(crate) fn declare(mut caps: HostCaps) -> HostCaps {
    let mut out = HostCaps::new();
    // The polyglot executor is not a handler; carry it over (CRUSH-271).
    if let Some(executor) = caps.take_polyglot_executor() {
        out.set_polyglot_executor(executor);
    }
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
    fn renamed_caps_are_refused_with_their_replacement() {
        assert_eq!(super::renamed_to("http.get"), Some("net.http_get"));
        assert_eq!(super::renamed_to("net.http_get"), None);
        let renamed = super::MissingGrant {
            capability: "http.get".into(),
            grant: None,
            renamed_to: Some("net.http_get"),
        };
        let text = super::describe_missing(&[renamed]);
        assert!(text.starts_with("  http.get was renamed to net.http_get"), "{text}");
        #[cfg(feature = "net")]
        assert!(text.ends_with("(grant it with --net)"), "{text}");
    }

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

    #[test]
    fn ui_caps_point_at_crush_gui() {
        let program =
            crate::compile::compile_crush_source("fn main() { ui.render(\"x\"); return 0 }").unwrap();
        let missing = super::missing_grants(&program, None).unwrap();
        assert_eq!(missing.len(), 1);
        assert!(missing[0].grant.is_some_and(|g| g.starts_with("--ui")), "{missing:?}");
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
        #[cfg(feature = "tcp")]
        {
            b = b.tcp(true).tcp_listen(true);
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
