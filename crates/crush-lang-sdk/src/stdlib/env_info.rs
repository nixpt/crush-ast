//! `env.os` / `env.arch` — compile-time platform constants.
//!
//! Ported from exosphere `core/base/stdlib/src/env.rs`. These read no
//! environment state (they are `std::env::consts`), so they are pure stdlib.
//! Reading environment *variables* is `env.get`, a grant-gated host
//! capability (`--env`) — see `host_caps.rs`.

use crush_vm::vm::Value;
use crush_vm::{HostCap, HostCapSpec, HostCaps};

pub(super) fn register(caps: &mut HostCaps) {
    caps.register(Box::new(EnvOsCap));
    caps.register(Box::new(EnvArchCap));
}

std_cap!(EnvOsCap, "env.os", Some(0), |_args: &[Value]| {
    Ok(Some(Value::Str(std::env::consts::OS.to_string())))
});

std_cap!(EnvArchCap, "env.arch", Some(0), |_args: &[Value]| {
    Ok(Some(Value::Str(std::env::consts::ARCH.to_string())))
});

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_the_host_platform() {
        let mut caps = HostCaps::new();
        register(&mut caps);
        let os = caps.get("env.os").unwrap().call(vec![]).unwrap();
        let arch = caps.get("env.arch").unwrap().call(vec![]).unwrap();
        assert_eq!(os, Some(Value::Str(std::env::consts::OS.into())));
        assert_eq!(arch, Some(Value::Str(std::env::consts::ARCH.into())));
    }
}
