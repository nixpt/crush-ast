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
        // Always registered: parses its argument, nothing else.
        "cson.parse" => &[],
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
